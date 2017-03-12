use connection::{Connection,ConnectionState};
use buffer::Buffer;

use std::io::{Error,ErrorKind,Read,Result,Write};

impl<'a> Connection<'a> {
  pub fn run<T>(&mut self, stream: &mut T, send_buffer: &mut Buffer, receive_buffer: &mut Buffer) -> Result<ConnectionState>
    where T: Read + Write {

    let mut write_would_block = false;
    let mut read_would_block  = false;

    loop {
      let continue_writing = !write_would_block && self.can_write(send_buffer);
      let continue_reading = !read_would_block && self.can_read(receive_buffer);
      let continue_parsing = self.can_parse(receive_buffer);

      if !continue_writing && !continue_reading && !continue_parsing {
        return Ok(self.state);
      }

      if continue_writing {
        match self.write_to_stream(stream, send_buffer) {
          Ok((sz,_)) => {

          },
          Err(e) => {
            match e.kind() {
              ErrorKind::WouldBlock => {
                write_would_block = true;
              },
              k => {
                println!("error writing: {:?}", k);
                self.state = ConnectionState::Error;
                return Err(e);
              }
            }
          }
        }
      }

      if continue_reading {
        match self.read_from_stream(stream, receive_buffer) {
          Ok(_) => {},
          Err(e) => {
            match e.kind() {
              ErrorKind::WouldBlock => {
                read_would_block = true;
              },
              k => {
                println!("error reading: {:?}", k);
                self.state = ConnectionState::Error;
                return Err(e);
              }
            }
          }
        }
      }

      if continue_parsing {
        //FIXME: handle the Incomplete case. We need a WantRead and WantWrite signal
        if let Ok((sz, state)) = self.parse(receive_buffer.data()) {
          receive_buffer.consume(sz);
        }
      }
    }

    let res:Result<ConnectionState> = Ok(self.state);
    res
  }

  pub fn can_write(&self, send_buffer: &Buffer) -> bool {
    send_buffer.available_data() > 0 || !self.frame_queue.is_empty()
  }

  pub fn can_read(&self, receive_buffer: &Buffer) -> bool {
    receive_buffer.available_space() > 0
  }

  pub fn can_parse(&self, receive_buffer: &Buffer) -> bool {
    receive_buffer.available_data() > 0
  }

  pub fn write_to_stream(&mut self, writer: &mut Write, send_buffer: &mut Buffer) -> Result<(usize, ConnectionState)> {
    match self.serialize(send_buffer.space()) {
      Ok((sz, _)) => {
        send_buffer.fill(sz);
      },
      Err(e) => {
        return Err(e);
      }
    }

    match writer.write(&mut send_buffer.data()) {
      Ok(sz) => {
        println!("wrote {} bytes", sz);
        send_buffer.consume(sz);
        Ok((sz, self.state))
      },
      Err(e) => Err(e),
    }
  }

  pub fn read_from_stream(&mut self, reader: &mut Read, receive_buffer: &mut Buffer) -> Result<(usize, ConnectionState)> {
    if self.state == ConnectionState::Initial || self.state == ConnectionState::Error {
      self.state = ConnectionState::Error;
      return Err(Error::new(ErrorKind::Other, "invalid state"))
    }

    match reader.read(&mut receive_buffer.space()) {
      Ok(sz) => {
        println!("read {} bytes", sz);
        receive_buffer.fill(sz);
        Ok((sz, self.state))
      },
      Err(e) => Err(e),
    }
  }
}