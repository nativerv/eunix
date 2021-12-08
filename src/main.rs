mod machine; 
mod eunix; 
mod util; 

use machine::Machine;

pub fn main() {
  let machine = Machine::new("../machines/1/machine.yaml");
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fs_create() {

  }
}

// vim:ts=2 sw=2
