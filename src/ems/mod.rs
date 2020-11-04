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
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub mod c_binding;

fn init_admin_connection() -> usize{
  let username = env::var("USERNAME").unwrap();
  let password = env::var("PASSWORD").unwrap();
  let server_url = env::var("SERVER_URL").unwrap();
  let c_server_url = CString::new(server_url).unwrap();
  let c_username = CString::new(username).unwrap();
  let c_password = CString::new(password).unwrap();
  let mut error = tibemsErrorContext{_private:[]};
  let err: *mut c_void = &mut error as *mut _ as *mut c_void;
  let mut admin:usize = 0;
  unsafe{
    let ssl = tibemsSSLParams_Create();
    let response = tibemsErrorContext_Create(err);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsErrorContext_Create {:?}",response);
    }
    
    let response = tibemsAdmin_Create(&mut admin,c_server_url.as_ptr(),c_username.as_ptr(),
      c_password.as_ptr(),ssl);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsAdmin_Create {:?}",response);
    } 
    //check for increased timeout setting
    match env::var("ADMIN_COMMAND_TIMEOUT_MS") {
      Ok(timeout) => {
        println!("setting command timeout to {}",timeout);
        let timeout_number = timeout.parse::<i64>();
        match timeout_number {
          Ok(nr) =>{
            let response = tibemsAdmin_SetCommandTimeout(admin,nr);
            if response != tibems_status::TIBEMS_OK {
              println!("tibemsAdmin_SetCommandTimeout {:?}",response);
            }    
          },
          Err(_err)=> {}
        }
      },
      Err(_err) => {}
    }
  }
  println!("creating admin connection");
  return admin
}

pub static TOPIC_ADMIN_CONNECTION: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(init_admin_connection()));
pub static QUEUE_ADMIN_CONNECTION: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(init_admin_connection()));

#[derive(Clone, Serialize, Deserialize)]
pub struct QueueInfo{
  pub queue_name: String,
  pub pending_messages: i64,
  pub consumers: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TopicInfo{
  pub topic_name: String,
  pub pending_messages: i64,
  pub subscribers: i32,
  pub durables: i32,
}

pub fn get_queue_stats() -> Vec<QueueInfo> {
  let start = Instant::now();
  let mut result = Vec::new();
  unsafe{
    let admin: usize = *QUEUE_ADMIN_CONNECTION.lock().unwrap();
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
    let response = tibemsDestinationInfo_Destroy(*col_ptr);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsDestinationInfo_Destroy {:?}",response);
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
      let response = tibemsDestinationInfo_Destroy(*col_ptr);
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
    let response = tibemsCollection_Destroy(dest_collection);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsCollection_Destroy {:?}",response);
    }  
  }
  let duration = start.elapsed();
  println!("{{\"queue_stats_duration_ms\": {} }}", duration.as_millis());
  return result;
}

pub fn get_topic_stats() -> Vec<TopicInfo> {
  let start = Instant::now();
  let mut result: Vec<TopicInfo> = Vec::new();
  unsafe{
    let admin: usize = *TOPIC_ADMIN_CONNECTION.lock().unwrap();
    //get all destinations
    let pattern = CString::new(">").unwrap();
    let mut dest_collection: usize = 1;
    let response = tibemsAdmin_GetDestinations(admin,&mut dest_collection,pattern.as_ptr(),
      tibemsDestinationType::TIBEMS_TOPIC,tibems_permType::TIBEMS_DEST_GET_NOTEMP, tibems_bool::TIBEMS_FALSE);
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
    let mut subscribers:i64=0;
    let response = tibemsTopicInfo_GetSubscriptionCount(*col_ptr,&mut subscribers);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsTopicInfo_GetSubscriptionCount {:?}",response);
    }
    let mut durables:i64=0;
    let response = tibemsTopicInfo_GetDurableCount(*col_ptr,&mut durables);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsTopicInfo_GetDurableCount {:?}",response);
    }
    let response = tibemsDestinationInfo_Destroy(*col_ptr);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsDestinationInfo_GetConsumerCount {:?}",response);
    }
    let destination = TopicInfo{
      topic_name: dest_name.to_string(),
      pending_messages: pending,
      subscribers: subscribers as i32,
      durables: durables as i32,
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
      let mut subscribers:i64=0;
      let response = tibemsTopicInfo_GetSubscriptionCount(*col_ptr,&mut subscribers);
      if response != tibems_status::TIBEMS_OK {
        println!("tibemsTopicInfo_GetSubscriptionCount {:?}",response);
      }
      let mut durables:i64=0;
      let response = tibemsTopicInfo_GetDurableCount(*col_ptr,&mut durables);
      if response != tibems_status::TIBEMS_OK {
        println!("tibemsTopicInfo_GetDurableCount {:?}",response);
      }
      let response = tibemsDestinationInfo_Destroy(*col_ptr);
      if response != tibems_status::TIBEMS_OK {
        println!("tibemsDestinationInfo_GetConsumerCount {:?}",response);
      }
      let destination = TopicInfo{
        topic_name: dest_name.to_string(),
        pending_messages: pending,
        subscribers: subscribers as i32,
        durables: durables as i32,
      };
      result.push(destination);
    }
    let response = tibemsCollection_Destroy(dest_collection);
    if response != tibems_status::TIBEMS_OK {
      println!("tibemsCollection_Destroy {:?}",response);
    }  
  }
  let duration = start.elapsed();
  println!("{{\"topic_stats_duration_ms\": {} }}", duration.as_millis());
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