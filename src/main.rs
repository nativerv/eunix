mod machine; 
mod eunix; 
mod util; 

use std::path::Path;

use machine::{Machine, OperatingSystem};

pub fn main() {
  let machine = Machine::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("machines/1/machine.yaml").to_str().unwrap());
  let os = OperatingSystem {
    kernel: eunix::kernel::Kernel::new(machine.get_devices()),
  };

  println!("Machine: {:?}", machine);
  println!();
  println!("OS: {:?}", os);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fs_create() {

  }
}

// vim:ts=2 sw=2
