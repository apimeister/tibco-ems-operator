use std::process::Command;
use std::env;
use std::fs::File;
use std::time::Instant;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};
use std::io::prelude::*;
use std::ffi::CString;
use std::ffi::CStr;
use std::ffi::c_void;
use c_binding::*;

pub mod c_binding;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueInfo{
  pub queue_name: String,
  pub pending_messages: i64,
  pub consumers: usize,
}

pub fn get_queue_stats() -> Vec<QueueInfo> {
  let start = Instant::now();
  let username = env::var("USERNAME").unwrap();
  let password = env::var("PASSWORD").unwrap();
  let server_url = env::var("SERVER_URL").unwrap();
  let c_server_url = CString::new(server_url).unwrap();
  let c_username = CString::new(username).unwrap();
  let c_password = CString::new(password).unwrap();
  let mut result = Vec::new();
  unsafe{
    //create error context
    let mut error = tibemsErrorContext{_private:[]};
    let err: *mut c_void = &mut error as *mut _ as *mut c_void;
    let ssl = tibemsSSLParams_Create();
    let response = tibemsErrorContext_Create(err);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsErrorContext_Create {:?}",response);
    }
    let mut admin:usize = 0;
    let response = tibemsAdmin_Create(&mut admin,c_server_url.as_ptr(),c_username.as_ptr(),
    c_password.as_ptr(),ssl);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsAdmin_Create {:?}",response);
    }
    //get all destinations
    let pattern = CString::new(">").unwrap();
    let mut dest_collection: usize = 1;
    let response = tibemsAdmin_GetDestinations(admin,&mut dest_collection,pattern.as_ptr(),
      tibemsDestinationType::TIBEMS_QUEUE,tibems_permType::TIBEMS_DEST_GET_NOTEMP, tibems_bool::TIBEMS_FALSE);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsAdmin_GetDestinations {:?}",response);
    }
    //get first entry
    let mut x:usize = 1;
    let col_ptr: *mut usize =  &mut x;
    let response = tibemsCollection_GetFirst(dest_collection, col_ptr);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsCollection_GetFirst {:?}",response);
    }
    let buf_size:usize  = 1024;
    let dest_vec:Vec<i8> = vec![0; buf_size];
    let response = tibemsDestinationInfo_GetName(*col_ptr, dest_vec.as_ptr(), buf_size);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsDestinationInfo_GetName {:?}",response);
    }
    let dest_name = CStr::from_ptr(dest_vec.as_ptr()).to_str().unwrap();
    let mut pending:i64 = 0;
    let response = tibemsDestinationInfo_GetPendingMessageCount(*col_ptr, &mut pending);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsDestinationInfo_GetPendingMessageCount {:?}",response);
    }
    let mut consumers:usize=255;
    let response = tibemsDestinationInfo_GetConsumerCount(*col_ptr,&mut consumers);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsDestinationInfo_GetConsumerCount {:?}",response);
    }
    let destination = QueueInfo{
      queue_name: dest_name.to_string(),
      pending_messages: pending,
      consumers: consumers
    };
    result.push(destination);

    //loop other entries
    while response != tibems_status::TIBEMS_NOT_FOUND {
      let response = tibemsCollection_GetNext(dest_collection, col_ptr);
      if response != tibems_status::TIBEMS_OK {
        if response == tibems_status::TIBEMS_NOT_FOUND {
          break;
        }
        println!("tibemsCollection_GetNext {:?}",response);
      }
      let buf_size:usize  = 1024;
      let dest_vec:Vec<i8> = vec![0; buf_size];
      let response = tibemsDestinationInfo_GetName(*col_ptr, dest_vec.as_ptr(), buf_size);
      if response != tibems_status::TIBEMS_OK {
        println!("tibemsDestinationInfo_GetName {:?}",response);
      }
      let dest_name = CStr::from_ptr(dest_vec.as_ptr()).to_str().unwrap();
      let mut pending:i64 = 0;
      let response = tibemsDestinationInfo_GetPendingMessageCount(*col_ptr, &mut pending);
      if response != tibems_status::TIBEMS_OK {
        println!("tibemsDestinationInfo_GetPendingMessageCount {:?}",response);
      }
      let mut consumers:usize=255;
      let response = tibemsDestinationInfo_GetConsumerCount(*col_ptr,&mut consumers);
      if response != tibems_status::TIBEMS_OK {
        println!("tibemsDestinationInfo_GetConsumerCount {:?}",response);
      }
      let destination = QueueInfo{
        queue_name: dest_name.to_string(),
        pending_messages: pending,
        consumers: consumers
      };
      result.push(destination);
      // println!("{}: pending_message: {} consumers: {}",dest_name,pending,consumers);
    }
  }
  let duration = start.elapsed();
  println!("get_queue_stats took: {:?}", duration);
  return result;
}

pub fn run_tibems_script(script: String) -> String{
  let start = Instant::now();
  let sys_time = SystemTime::now();
  let sys_millis = sys_time.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
  let filename = format!("/tmp/{}.script",sys_millis);
  let mut file = File::create(&filename).unwrap();
  file.write_all(&script.into_bytes()).unwrap();
  let username = env::var("USERNAME").unwrap();
  let password = env::var("PASSWORD").unwrap();
  let server_url = env::var("SERVER_URL").unwrap();
  let p = Command::new("tibemsadmin")
      .arg("-user")
      .arg(username)
      .arg("-password")
      .arg(password)
      .arg("-server")
      .arg(server_url)
      .arg("-module_path")
      .arg("/usr/lib64")
      .arg("-script")
      .arg(&filename)
      .output().unwrap();
  let w = p.stdout;
  let x = String::from_utf8(w).unwrap();
  let _ignore = std::fs::remove_file(filename);
  let duration = start.elapsed();
  println!("run_tibems_script took: {:?}", duration);
  return x;
}