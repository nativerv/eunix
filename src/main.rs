use std::fs;
use serbia::serbia;
use serde::{Deserialize, Serialize};
use std::fs::File;

const BLOCK_SIZE: usize = 512;
//const DISK_PATH: = "./disk.enxvd";

#[derive(Serialize, Deserialize)]
struct Superblock {
  num_inodes: usize,
  num_blocks: i32,
  block_size: usize,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct INode<'a> {
  size: i32,
  name: &'a str,
}

#[serbia]
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Block {
  size: i32,
  data: [u8; BLOCK_SIZE],
  next_block_idx: i32,
}

#[derive(Serialize, Deserialize)]
struct FS<'a> {
  superblock: Superblock,

  #[serde(borrow)]
  inodes: Vec<Box<INode<'a>>>,

  blocks: Vec<Box<Block>>,
}

impl FS<'_> {
  fn new() -> Self { 
    let superblock = Superblock {
      num_inodes: 10,
      num_blocks: 100,
      block_size: std::mem::size_of::<Block>(),
    };

    let inodes = vec![Box::new(INode {
      size: -1,
      name: "",
    }); superblock.num_inodes];

    let blocks = vec![Box::new(Block {
      size: -1,
      data: [0; BLOCK_SIZE],
      next_block_idx: -1,
    }); superblock.num_inodes];

    Self { 
      superblock,
      inodes,
      blocks: Vec::new(),
    } 
  }

  fn mount() {
  }

  fn get_as_bytes(&self) -> Vec<u8> {
    return bincode::serialize(self).unwrap();
  }

  fn sync(&self) {
    let bytes = self.get_as_bytes();
    fs::write("./disk.enxvd", &bytes).unwrap();
  }
}

pub fn main() {
  let fs = FS::new();
  fs.sync();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fs_create() {
    let fs = FS::new();

    println!("Hex: {:02X?}", fs.get_as_bytes());
    //println!("{:?}", fs.get_as_bytes());
    println!("Dec: {:?}", bincode::serialize(&fs.superblock).unwrap());
  }
}

