use std::any::{Any, TypeId};
use super::*;
use std::process::Command;

pub fn mkenxvd(size: String, file_path: String) {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/mkenxvd.sh");

  assert!(path.exists());

  Command::new(path.to_str().unwrap())
    .arg(size)
    .arg(file_path)
    .spawn()
    .unwrap()
    .wait()
    .unwrap();
}

pub fn mktemp() -> String {
  String::from_utf8(
    Command::new("sh")
      .arg("-c")
      .arg("mktemp")
      .output()
      .unwrap()
      .stdout,
  )
  .unwrap()
}

pub fn unixtime() -> u32 {
  std::time::SystemTime::now()
    .duration_since(std::time::SystemTime::UNIX_EPOCH)
    .unwrap()
    .as_secs()
    .try_into()
    .unwrap()
}

pub fn fixedpoint<F, T>(f: F, initial: T) -> T
  where
    F: Fn(&T) -> T,
    T: PartialEq,
  
{
  let mut result = f(&initial);

  while result != f(&result) {
    result = f(&result);
  }

  result
}

// vim:ts=2 sw=2
