use std::process::Command;
use std::env;
use std::fs::File;
use std::io::prelude::*;

pub fn run_tibems_script(script: String) -> String{
  let sys_time = std::time::SystemTime::now();
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
  return x;
}