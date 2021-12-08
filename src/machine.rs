use std::path::Path;
use std::str::FromStr;
use serde::{Serialize, Deserialize};

use crate::eunix::fs::AddressSize;
use crate::eunix::kernel::Kernel;
use std::collections::BTreeMap;

pub struct DirectoryEntry<'a> {
  inode_address: AddressSize,
  name: &'a str,
  next_dir_entry_offset: AddressSize,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum VirtualDevice {
  BlockDevice,
  TTYDevice,
}

// pub trait VirtualDevice: InstanceOf {
//   fn get_path(&self) -> Path;
// }
//
// pub struct BlockVirtualDevice {
//   path: Path,
// }
// impl VirtualDevice for BlockVirtualDevice {
//   fn get_path(&self) -> Path {
//     self.path
//   }
// }

#[derive(Debug)]
pub struct OperatingSystem<'a> {
  pub kernel: Kernel<'a>,
}

pub type DeviceTable = BTreeMap<String, VirtualDevice>; 

#[derive(Debug)]
pub struct Machine {
  devices: DeviceTable,
  is_booted: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineSchema {
  machine: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
}

impl Machine {
  pub fn new(machine_schema_path: &str) -> Self {
    let machine_schema_reader = std::fs::File::open(machine_schema_path)
      .unwrap();

    let machine_schema = 
      serde_yaml::from_reader::<_, MachineSchema>(machine_schema_reader)
        .unwrap();

    let devices: DeviceTable = machine_schema.machine
      .get("devices")
      .unwrap()
      .into_iter()
      .map(|(_name, device)| {
        let device_path = Path::new(&machine_schema_path).join(device.get("path").unwrap());
        let device_type = device.get("type").unwrap();

        let a = String::from_str(device_path.to_str().unwrap()).unwrap();
        (a, match device_type.as_ref() {
          "block" => VirtualDevice::BlockDevice,
          "tty" => VirtualDevice::BlockDevice,
          _ => panic!(),
        })
      })
      .collect();

    Self {
      is_booted: false,
      devices,
    }
  }
  pub fn get_devices(&self) -> &DeviceTable {
    &self.devices
  }
  pub fn run(&self, os: OperatingSystem) {
  }
}

// https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
// https://stackoverflow.com/questions/52005382/what-is-a-function-for-structs-like-javas-instanceof


// vim:ts=2 sw=2
