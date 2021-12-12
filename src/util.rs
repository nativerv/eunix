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

pub trait InstanceOf
where
    Self: Any,
{
    fn instance_of<U: ?Sized + Any>(&self) -> bool {
        TypeId::of::<Self>() == TypeId::of::<U>()
    }
}

// implement this trait for every type that implements `Any` (which is most types)
impl<T: ?Sized + Any> InstanceOf for T {}

pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
::std::slice::from_raw_parts(
    (p as *const T) as *const u8,
    ::std::mem::size_of::<T>(),
)
}
