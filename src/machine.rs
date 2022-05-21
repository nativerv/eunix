use std::path::Path;
use std::str::FromStr;
use serde::{Serialize, Deserialize};

use crate::eunix::fs::AddressSize;
use crate::eunix::kernel::Kernel;
use std::collections::BTreeMap;


#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum VirtualDeviceType {
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
pub struct OperatingSystem {
  pub kernel: Kernel,
}

#[derive(Debug, Clone)]
pub struct MachineDeviceTable {
  pub devices: BTreeMap<String, VirtualDeviceType>,
}
// /// realpath -> (dev_type, pathname) 
// pub type DeviceTable = BTreeMap<String, (VirtualDeviceType, Option<String>)>; 

#[derive(Debug)]
pub struct Machine {
  device_table: MachineDeviceTable,
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

    let devices = MachineDeviceTable { 
      devices: machine_schema.machine
      .get("devices")
      .unwrap()
      .into_iter()
      .map(|(_name, device)| {
        let device_path = Path::new(&machine_schema_path).parent().unwrap().join(device.get("path").unwrap());
        let device_type = device.get("type").unwrap();

        let a = String::from_str(device_path.to_str().unwrap()).unwrap();
        (a, match device_type.as_ref() {
          "block" => VirtualDeviceType::BlockDevice,
          "tty" => VirtualDeviceType::TTYDevice,
          _ => panic!("machine: can't start: unknown device type in {}", machine_schema_path),
        })
      })
      .collect()
    };

    Self {
      is_booted: false,
      device_table: devices,
    }
  }
  pub fn device_table(&self) -> &MachineDeviceTable {
    &self.device_table
  }
  pub fn run(&self, os: OperatingSystem) {
  }
}

// https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
// https://stackoverflow.com/questions/52005382/what-is-a-function-for-structs-like-javas-instanceof

#[cfg(test)]
mod tests {
    use crate::util::{mktemp, mkenxvd};

  #[test]
  fn lookup_path_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    // let e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();
    //
    // let kernel = Kernel::new();
    //
    // let mut mount_points = BTreeMap::new(); 
    // mount_points.insert(String::from("/"), MountedFilesystem {
    //   r#type: RegisteredFilesystem::e5fs,
    //   driver: Box::new(e5fs),
    // });
    // mount_points.insert(String::from("/dev"), MountedFilesystem {
    //   r#type: RegisteredFilesystem::devfs,
    //   driver: Box::new(DeviceFilesystem::new(&crate::eunix::kernel::KernelDeviceTable { devices:  }),
    //                    });
    //
    //   let mut vfs = VFS {
    //     open_files: BTreeMap::new(),
    //     mount_points,
    //   };
    //
    // let dev_dir = vfs.read_dir("/dev").unwrap();
    //
  }
}

// vim:ts=2 sw=2
