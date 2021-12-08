use std::fs;
use serde::{Deserialize, Serialize};

const BLOCK_DATA_SIZE: usize = 512;
static DISK_PATH: &str = "./disk.enxvd";

#[derive(Serialize, Deserialize)]
#[repr(C)]
struct Superblock {
  num_inodes: usize,
  num_blocks: i32,
  block_size: usize,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
struct INode<'a> {
  size: i32,
  name: &'a str,
}

#[allow(non_snake_case)]
#[derive(Clone, Serialize, Deserialize)]
#[repr(C)]
struct Block {
  size: i32,
  data: Vec<u8>,
  next_block_idx: i32,
}

#[derive(Serialize, Deserialize)]
#[repr(C)]
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
      data: vec![0; BLOCK_DATA_SIZE],
      next_block_idx: -1,
    }); superblock.num_inodes];

    Self { 
      superblock,
      inodes,
      blocks,
    } 
  }

  fn mount() {
  }

  fn get_as_bytes(&self) -> Vec<u8> {
    return bincode::serialize(self).unwrap();
  }

  fn sync(&self) {
    let bytes = self.get_as_bytes();
    fs::write(DISK_PATH, &bytes).unwrap();
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

    // println!("Hex: {:02X?}", fs.get_as_bytes());
    //println!("{:?}", fs.get_as_bytes());
    println!("&fs.superblock.num_inodes: {:?}", bincode::serialize(&fs.superblock.num_inodes).unwrap());
    println!("&fs.superblock.num_blocks: {:?}", bincode::serialize(&fs.superblock.num_blocks).unwrap());
    println!("&fs.superblock.block_size: {:?}", bincode::serialize(&fs.superblock.block_size).unwrap());

    println!("&fs.inodes[0].name: {:?}", bincode::serialize(&fs.inodes[0].name).unwrap());
    println!("&fs.inodes[0].size: {:?}", bincode::serialize(&fs.inodes[0].size).unwrap());

    println!("&fs.blocks[0].size: {:?}", bincode::serialize(&fs.blocks[0].size).unwrap());
    println!("&fs.blocks[0].data: {:?}", bincode::serialize(&fs.blocks[0].data).unwrap());
  }
}

// vim:ts=2 sw=2
