use lapin_async::api::RequestId;

use std::default::Default;
use std::io::{self,Error,ErrorKind};
use futures::{Async,Future};
use futures::future;
use tokio_io::{AsyncRead,AsyncWrite};
use std::sync::{Arc,Mutex};
use std::time::Duration;

use transport::*;
use channel::Channel;

/// the Client structures connects to a server and creates channels
#[derive(Clone)]
pub struct Client<T> {
    transport: Arc<Mutex<AMQPTransport<T>>>,
}

#[derive(Clone,Debug,PartialEq)]
pub struct ConnectionOptions {
  pub username:  String,
  pub password:  String,
  pub heartbeat: u16,
}

impl Default for ConnectionOptions {
  fn default() -> ConnectionOptions {
    ConnectionOptions {
      username:  "guest".to_string(),
      password:  "guest".to_string(),
      heartbeat: 60,
    }
  }
}

impl<T: AsyncRead+AsyncWrite+'static> Client<T> {
  /// takes a stream (TCP, TLS, unix socket, etc) and uses it to connect to an AMQP server.
  ///
  /// this method returns a future that resolves once the connection handshake is done.
  /// The result is a client that can be used to create a channel
  pub fn connect(stream: T, options: &ConnectionOptions) -> Box<Future<Item = Client<T>, Error = io::Error>> {
    Box::new(AMQPTransport::connect(stream.framed(AMQPCodec), options).and_then(|transport| {
      debug!("got client service");
      let client = Client {
        transport: Arc::new(Mutex::new(transport)),
      };

      future::ok(client)
    }))

  }

  /// creates a new channel
  ///
  /// returns a future that resolves to a `Channel` once the method succeeds
  pub fn create_channel(&self) -> Box<Future<Item = Channel<T>, Error = io::Error>> {
    let channel_transport = self.transport.clone();

    if let Ok(mut transport) = self.transport.lock() {
      let channel_id: u16 = transport.conn.create_channel();
      match transport.conn.channel_open(channel_id, "".to_string()) {
        //FIXME: should use errors from underlying library here
        Err(e) => Box::new(
          future::err(Error::new(ErrorKind::ConnectionAborted, format!("could not create channel: {:?}", e)))
        ),
        Ok(request_id) => {
          trace!("request id: {}", request_id);
          transport.send_frames();
          transport.handle_frames();

          //FIXME: very afterwards that the state is Connected and not error
          Box::new(wait_for_answer(channel_transport.clone(), request_id).map(move |_| {
            Channel {
              id:        channel_id,
              transport: channel_transport,
            }
          }))
        }
      }
    } else {
      //FIXME: if we're there, it means the mutex failed
      Box::new(future::err(
        Error::new(ErrorKind::ConnectionAborted, format!("could not create channel"))
      ))
    }
  }

}

/// internal method to wait until a specific request succeeded
pub fn wait_for_answer<T: AsyncRead+AsyncWrite+'static>(transport: Arc<Mutex<AMQPTransport<T>>>, request_id: RequestId) -> Box<Future<Item = (), Error = io::Error>> {
  Box::new(future::poll_fn(move || {
    let connected = if let Ok(mut tr) = transport.try_lock() {
      tr.handle_frames();
      if ! tr.conn.is_finished(request_id) {
        //retry because we might have obtained a new frame
        tr.handle_frames();
        tr.conn.is_finished(request_id)
      } else {
        true
      }
    } else {
      return Ok(Async::NotReady);
    };

    if connected {
      Ok(Async::Ready(()))
    } else {
      Ok(Async::NotReady)
    }
  }))

}
