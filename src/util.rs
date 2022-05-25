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

pub fn unixtime() -> u64 {
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

/// Gets the bit at position `n`.
/// Bits are numbered from 0 (least significant) to 7 (most significant).
pub fn get_bit_at(input: u8, n: u8) -> bool {
  if n < 8 {
    input & (1 << n) != 0
  } else {
    false
  }
}

/// Split /usr/bin:/bin -> ["/usr/bin", "/bin"]
pub fn split_path_variable<'a>(path_variable: &'a str) -> impl Iterator<Item = &'a str> + 'a {
  path_variable.split::<'a>(':')
}

// vim:ts=2 sw=2
