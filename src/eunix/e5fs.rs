use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Write;
use std::slice::SliceIndex;

use fancy_regex::Regex;

use crate::eunix::fs::FileModeType;
use crate::eunix::fs::NOBODY;
use crate::util::fixedpoint;
use crate::util::unixtime;

use super::fs::AddressSize;
use super::fs::FileMode;
use super::fs::FileStat;
use super::fs::Filesystem;
use super::fs::Id;
use super::fs::NO_ADDRESS;
use super::fs::VDirectory;
use super::fs::VDirectoryEntry;
use super::fs::VINode;
use super::fs;
use super::kernel::Errno;

/* 
 * LEGEND: 
 * fbl       - free blocks list, the reserved blocks at the
 *             end of the blocks list which contain free
 *             block numbers for quick allocation
 * fbl_chunk - vector of numbers parsed from fbl block
 * */

#[derive(Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
  pub inode_number: AddressSize,
  pub rec_len: u16,
  pub name_len: u8,
  pub name: String,
}

impl DirectoryEntry {
  fn new(inode_number: AddressSize, name: &str) -> Result<Self, Errno> {
    use std::mem::size_of;

    Ok(Self {
      inode_number,
      rec_len: (size_of::<AddressSize>() + size_of::<u16>() + size_of::<u8>() + name.len()) as u16,
      name_len: name.len().try_into().or_else(|_| Err(Errno::EINVAL("DirectoryEntry::new: name can't be bigger than 255")))?,
      name: name.to_owned(),
    })
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Directory {
  entries_count: AddressSize,
  pub entries: BTreeMap<String, DirectoryEntry>,
}

impl Directory {
  pub fn new() -> Self {
    Self {
      entries_count: 0,
      entries: BTreeMap::new(),
    }
  }
  fn from(entries: BTreeMap<String, DirectoryEntry>) -> Self {
    println!("[Directory::from]: entries_count: {:?}", entries.len() as AddressSize);
    println!("[Directory::from]: entries: {:?}", entries);

    Self {
      entries_count: entries.len() as AddressSize,
      entries,
    }
  }

  // pub fn entries_count(&self) -> AddressSize {
  //   self.entries_count
  // }
  // pub fn entries(&self) -> BTreeMap<String, DirectoryEntry> {
  //   self.entries
  // }

  pub fn insert(&mut self, inode_number: AddressSize, name: &str) -> Result<(), Errno> {
    self.entries.insert(
      name.to_owned(),
      DirectoryEntry::new(inode_number, name)?
    );
    self.entries_count += 1;
    Ok(())
  }
  pub fn remove(&mut self, name: &str) -> Result<(), Errno> {
    self.entries.remove(name).ok_or(Errno::ENOENT("no such file in directory"))?;
    self.entries_count -= 1;
    Ok(())
  }
}

// 2 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16)
// 2 + 8 + 4 + 4 + 8 + 4 + 4 + 4 + (8 * 16)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct INode {
  mode: FileMode,
  links_count: AddressSize,
  uid: Id,
  gid: Id,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
  direct_block_numbers: [AddressSize; 12],
  indirect_block_numbers: [AddressSize; 3],
  number: AddressSize,
}

impl From<INode> for VINode {
  fn from(inode: INode) -> Self {
    Self {
      mode: inode.mode,
      links_count: inode.links_count,
      file_size: inode.file_size,
      uid: inode.uid,
      gid: inode.gid,
      atime: inode.atime,
      ctime: inode.ctime,
      mtime: inode.mtime,
      number: inode.number,
    }
  }
}

impl From<DirectoryEntry> for VDirectoryEntry {
  fn from(entry: DirectoryEntry) -> Self {
    Self {
      inode_number: entry.inode_number,
      name: entry.name,
    }
  }
}

impl From<Directory> for VDirectory {
  fn from(dir: Directory) -> Self {
    Self {
      entries: dir.entries.into_iter().map(|(key, entry)| (key, entry.into())).collect(),
    }
  }
}

impl Default for INode {
  fn default() -> Self {
    Self {
      mode: FileMode::default(),
      links_count: 0,
      file_size: 0,
      uid: NOBODY,
      gid: NOBODY,
      atime: 0,
      mtime: 0,
      ctime: 0,
      direct_block_numbers: [NO_ADDRESS; 12],
      indirect_block_numbers: [NO_ADDRESS; 3],
      number: 0,
    }
  }
}

// 16 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16) + (4 * 16)
// 16 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + (8 * 16) + (8 * 16)
#[derive(Default, Debug, PartialEq, Clone, Copy)]
pub struct Superblock {
  pub filesystem_type: [u8; 16],
  pub filesystem_size: AddressSize, // in blocks
  pub inode_table_size: AddressSize,
  pub inode_table_percentage: f32,
  pub free_inodes_count: AddressSize,
  pub free_blocks_count: AddressSize,
  pub inodes_count: AddressSize,
  pub blocks_count: AddressSize,
  pub block_size: AddressSize,
  pub block_data_size: AddressSize,
  pub free_inode_numbers: [AddressSize; 16],
  pub first_fbl_block_number: AddressSize,
}


impl Superblock {
  fn size() -> AddressSize {
    std::mem::size_of::<Superblock>() as AddressSize
  }

  fn new(fs_info: &mut E5FSFilesystemBuilder) -> Self {
    let _superblock_size = fs_info.superblock_size;
    let filesystem_size = fs_info.filesystem_size;
    let inode_table_size = fs_info.inode_table_size;
    let inode_table_percentage = fs_info.inode_table_percentage;
    let _inode_size = fs_info.inode_size;
    let block_size = fs_info.block_size;
    let block_data_size = fs_info.block_data_size;
    let inodes_count = fs_info.inodes_count;
    let blocks_count = fs_info.blocks_count;

    let mut free_inodes = [0; 16];
    for i in 0..16 {
      free_inodes[i as usize] = if i < inodes_count {
        i
      } else {
        NO_ADDRESS
      }
    }

    Self {
      filesystem_type: [
        'e' as u8, '5' as u8, 'f' as u8, 's' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
      ],
      filesystem_size, // in blocks?
      inode_table_size,
      inode_table_percentage,
      free_inodes_count: inodes_count,
      free_blocks_count: blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      block_data_size,
      free_inode_numbers: free_inodes,
      first_fbl_block_number: fs_info.free_blocks_count,
    }
  }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Block {
  data: Vec<u8>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct E5FSFilesystemBuilder {
  realfile: std::fs::File,
  device_size: AddressSize,
  superblock_size: AddressSize,
  inode_size: AddressSize,
  block_size: AddressSize,
  inodes_count: AddressSize,
  blocks_count: AddressSize,
  inode_table_size: AddressSize,
  filesystem_size: AddressSize,
  blocks_needed_for_fbl: AddressSize,
  first_inode_address: AddressSize,
  first_block_address: AddressSize,
  block_table_size: AddressSize,
  block_data_size: AddressSize,
  free_blocks_count: AddressSize,
  address_size: AddressSize,
  block_numbers_per_fbl_chunk: AddressSize,
  inode_table_percentage: f32,
  first_flb_block_number: AddressSize,
  root_inode_number: AddressSize,
}

impl E5FSFilesystemBuilder {
  pub fn new(device_realpath: &str, inode_table_percentage: f32, block_data_size: AddressSize) -> Result<Self, &'static str> {
    // Guard for percent_inodes
    match inode_table_percentage {
      n if n < 0f32 => return Err("percent_inodes can't be less than 0"),
      n if n > 1f32 => return Err("percent_inodes can't be more than 1"),
      _ => (),
    };

    // Guard for block_data_size
    match block_data_size {
      n if (n as f64).log2().fract() != 0f64 => return Err("block_data_size must be power of 2"),
      n if n < 512 => return Err("block_data_size can't be less than 512"),
      _ => (),
    };

    let realfile = std::fs::OpenOptions::new()
      .read(true)
      .write(true)
      .open(device_realpath)
      .unwrap();

    let device_size = realfile.metadata().unwrap().len() as AddressSize;
    let superblock_size = Superblock::size();
    let inode_size = std::mem::size_of::<INode>() as AddressSize;

    let address_size = std::mem::size_of::<AddressSize>() as AddressSize;

    // next_block_number + data
    let block_size = block_data_size;

    let inodes_count = ((device_size as f32 * inode_table_percentage) / inode_size as f32) as AddressSize;
    let blocks_count =
      ((device_size as f32 * (1f32 - inode_table_percentage)) / block_size as f32) as AddressSize;

    let inode_table_size = inode_size * inodes_count;

    let filesystem_size = superblock_size + inode_table_size + block_size * blocks_count;

    let first_inode_address = superblock_size;
    let first_block_address = superblock_size + inode_table_size;

    // ceil(
    //   blocks_count / (block_data_size / block_address_size)
    // )
    let blocks_needed_for_fbl = (blocks_count as f64
      / (block_data_size as f64 / std::mem::size_of::<AddressSize>() as f64)).ceil() as AddressSize;

    let free_blocks_count = blocks_count - blocks_needed_for_fbl;

    let addresses_per_fbl_chunk = block_data_size / address_size;

    // Guard for not enough blocks even for free blocks list
    match blocks_needed_for_fbl {
      n if n >= blocks_count => return Err("disk size is too small: blocks_needed_for_fbl > blocks_count"),
      _ => (),
    }

    let block_table_size = block_size * blocks_count;

    let first_flb_block_number = free_blocks_count;

    Ok(Self {
      realfile,
      device_size,
      superblock_size,
      inode_size,

      block_size,

      inodes_count,
      blocks_count,

      inode_table_size,

      filesystem_size,
      blocks_needed_for_fbl,

      first_inode_address,
      first_block_address,

      block_table_size,
      block_data_size,

      free_blocks_count,
      address_size,
      block_numbers_per_fbl_chunk: addresses_per_fbl_chunk,
      inode_table_percentage,
      first_flb_block_number,
      root_inode_number: 0,
    })
  }
}

#[allow(dead_code)]
pub struct E5FSFilesystem {
  superblock: Superblock,
  fs_info: E5FSFilesystemBuilder,
}

impl Filesystem for E5FSFilesystem {
  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    // Regex matching final_component of path (+ leading slash)
    let (everything_else, dirent_name) = E5FSFilesystem::split_path(pathname)?;
    let dir_pathname = format!("/{}", everything_else.join("/"));
    //println!("dir_pathname: {}", dir_pathname);

    // Get dir path with this regex
    let dir_inode = self.lookup_path(dir_pathname.as_str())?;

    // Read dir from disk
    let mut dir = self.read_dir_from_inode(dir_inode.number)?;

    // Guard for file already
    if let Some(_) = dir.entries
      .iter()
      .find(|(name, _entry)| format!("/{}", name) == dirent_name)
    {
       return Err(Errno::ENOENT("file already exists"));
    }

    // Allocate inode
    let (_, inode) = self.allocate_file()?;

    // Push allocated to dir
    dir.insert(inode.number, dirent_name.as_str())?;

    // Write dir
    self.write_dir(&dir, dir_inode.number)?;

    Ok(inode.into())
  } 

  fn read_file(&mut self, pathname: &str, _count: AddressSize)
    -> Result<Vec<u8>, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    self.read_from_file(inode_number)
  }

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<VINode, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let new_vinode: VINode = self.write_to_file(data.to_owned(), inode_number, false)?.into();
    Ok(new_vinode)
  } 

  fn read_dir(&mut self, pathname: &str)
    -> Result<VDirectory, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let dir = self.read_dir_from_inode(inode_number)?;

    Ok(dir.into())
  }

  fn stat(&mut self, pathname: &str) 
    -> Result<FileStat, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let INode {
      mode,
      file_size,
      links_count,
      uid,
      gid,
      ..
    } = self.read_inode(inode_number);

    Ok(FileStat {
      mode,
      size: file_size,
      inode_number,
      links_count,
      uid,
      gid,
      block_size: self.fs_info.block_size,
    })
  }

  fn change_mode(&mut self, pathname: &str, mode: FileMode)
    -> Result<(), Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    self.write_mode(inode_number, mode)
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let pathname = E5FSFilesystem::split_path(pathname)?;
    let (everything_else, final_component) = pathname.clone();
    let mut inode: INode = self.read_inode(self.fs_info.root_inode_number);

    // Base case
    if pathname == (Vec::new(), String::from("/")) {
      let inode = self.read_inode(self.fs_info.root_inode_number);
      return Ok(inode.into());
    };

    fn is_dir(inode: VINode) -> bool {
      let filetype = inode.mode.r#type();
      //println!("FileModeType::Dir as u8: {}", FileModeType::Dir as u8);
      //println!("filetype: {}", filetype);
      filetype == FileModeType::Dir as u8
    }

    let mut find_dir = |e5fs: &mut E5FSFilesystem, everything_else: Vec<String>| -> Result<INode, Errno> {
      let mut everything_else = VecDeque::from(everything_else);
      // TODO: pass inode to read_dir_from_inode
      while everything_else.len() > 0 {
        if !is_dir(inode.into()) {
          return Err(Errno::EINVAL("e5fs.lookup_path: not a directory (find_dir)"))
        }

        let piece = everything_else.pop_front().unwrap();
        let dir = e5fs.read_dir_from_inode(inode.number)?;
        if let Some(entry) = dir.entries.get(&piece.to_owned()) {
          inode = e5fs.read_inode(entry.inode_number);
        } else {
          return Err(Errno::ENOENT("e5fs.lookup_path: no such file or directory"))
        }
      }

      Ok(inode)
    };

    // Try to find directory - "everything else" part of `pathname`
    let dir_inode = find_dir(self, everything_else)?;
    let dir = self.read_dir_from_inode(dir_inode.number)?;

    //println!("final_component: {}", final_component);
    //println!("dir_inode: {:#?}", dir_inode);
    //println!("dir: {:#?}", dir);
    
    // Try to find file in directory and map its INode to VINode -
    // "final component" part of `pathname`, then return it
    dir.entries
      .get(&final_component)
      // Read its inode_number
      .map(|entry| self.read_inode(entry.inode_number).into())
      .ok_or_else(|| Errno::ENOENT("e5fs.lookup_path: no such file or directory (get(final_component))"))
  } 

  fn get_name(&self)
    -> String {
    todo!();
  } 
}

impl E5FSFilesystem {
  /// Read filesystem from device (file on host) path
  pub fn from(device_realpath: &str) -> Result<Self, Errno> {
    let Superblock {
      inode_table_percentage,
      block_data_size,
      ..
    } = E5FSFilesystem::read_superblock(device_realpath);

    let mut fs_info = 
      E5FSFilesystemBuilder::new(
        device_realpath, 
        inode_table_percentage, 
        block_data_size
      )
      .unwrap();

    Ok(Self {
      superblock: Superblock::new(&mut fs_info),
      fs_info,
    })
  }

  /// Create new filesystem and write it to disk
  pub fn mkfs(device_realpath: &str, inode_table_percentage: f32, block_data_size: AddressSize) -> Result<Self, Errno> {
    let mut fs_info = E5FSFilesystemBuilder::new(
        device_realpath, 
        inode_table_percentage, 
        block_data_size
      )
      .unwrap();

    let mut e5fs = Self {
      superblock: Superblock::new(&mut fs_info),
      fs_info,
    };

    let superblock = Superblock::new(&mut e5fs.fs_info);

    // 1. Write Superblock
    e5fs.write_superblock(&superblock).unwrap();

    // 2. Write fbl (free_block_list)
    e5fs.write_fbl();

    // 3. Write root dir - first allocated file (inode) 
    //    will always be 0-th inode in inode table
    let (root_inode_number, _) = e5fs.allocate_file()?;
    e5fs.write_dir(&Directory::new(), root_inode_number)?;

    Ok(e5fs)
  }

  fn write_dir(&mut self, dir: &Directory, inode_number: AddressSize) -> Result<INode, Errno> {
    // Convert `Directory` to bytes
    let entries_count_bytes = dir.entries_count.to_le_bytes().as_slice().to_owned();
    let entries_bytes = dir.entries.iter().fold(Vec::new(), |mut bytes, (_name, entry)| {
      bytes.write(entry.inode_number.to_le_bytes().as_slice()).unwrap();
      bytes.write(entry.rec_len.to_le_bytes().as_slice()).unwrap();
      bytes.write(entry.name_len.to_le_bytes().as_slice()).unwrap();
      bytes.write(entry.name.as_bytes()).unwrap();
      bytes
    });

    println!("[E5FS::write_dir]: entries_count_bytes: {:?}", entries_count_bytes);
    println!("[E5FS::write_dir]: entries_bytes: {:?}", entries_bytes);

    // Write them to one `Vec`
    let mut data = Vec::new();
    data.write(&entries_count_bytes).or_else(|_| Err(Errno::EIO("write_dir: can't write entries_count_bytes to data")))?;
    data.write(&entries_bytes).or_else(|_| Err(Errno::EIO("write_dir: can't write entries_bytes to data")))?;
    println!("[E5FS::write_dir]: data: {:?}", data);
    //println!("data: {:?}", data);

    // Write `Vec` to file
    let new_inode = self.write_to_file(data, inode_number, false)?;

    Ok(new_inode)
  }

  fn read_dir_from_inode(&mut self, inode_number: AddressSize) -> Result<Directory, Errno> {
    let dir_bytes = self.read_from_file(inode_number)?;
    //println!("dir_bytes: {:?}", dir_bytes);
    let directory = E5FSFilesystem::parse_directory(&mut self.fs_info, dir_bytes)?;

    Ok(directory)
  }

  fn write_to_file(&mut self, data: Vec<u8>, inode_number: AddressSize, _append: bool) -> Result<INode, Errno> {
    let inode = self.read_inode(inode_number);

    // If data is greater than available in inode's blocks,
    // grow the file
    let difference = data.len() as isize - (self.get_inode_blocks_count(inode_number)? * self.fs_info.block_size) as isize;
    if  difference > 0 {
      self.grow_file(inode_number, (difference as f64 / self.fs_info.block_size as f64).ceil() as AddressSize)?;
    }

    // Split data to chunks and write it to inode's blocks
    let chunks = data.chunks(self.fs_info.block_size as usize).zip(0..);
    for (chunk, i) in chunks {
      self.write_block(&Block { data: chunk.to_owned(), }, inode.direct_block_numbers[i])?;
    };

    // Write new size to inode
    let mut inode_cloned = inode.clone();
    inode_cloned.file_size = data.len() as AddressSize;
    self.write_inode(&inode_cloned, inode_number)?;

    Ok(inode_cloned)
  }

  fn read_from_file(&mut self, inode_number: AddressSize) -> Result<Vec<u8>, Errno> {
    let inode = self.read_inode(inode_number);

    let data = inode.direct_block_numbers
      .iter()
      .take_while(|&&block_number| block_number != NO_ADDRESS)
      .fold(Vec::new(), |mut bytes, &block_number| {
        let block = self.read_block(block_number);
        //println!("dir_block #{}: {:?}", block_number, block.data);
        bytes.write(&block.data).unwrap();
        bytes
      });

    Ok(data)
  }

  fn get_inode_blocks_count(&mut self, inode_number: AddressSize) -> Result<AddressSize, Errno> {
    let inode = self.read_inode(inode_number);

    Ok(inode.direct_block_numbers
       .iter()
       .fold(0, |blocks_count, &block_number| {
         blocks_count + if block_number != NO_ADDRESS { 1 }  else { 0 }
       })
    )
  }

  fn read_mode(&mut self, inode_number: AddressSize) -> Result<FileMode, Errno> {
    let inode = self.read_inode(inode_number);
    Ok(inode.mode)
  }

  fn write_mode(&mut self, inode_number: AddressSize, mode: FileMode) -> Result<(), Errno> {
    let mut inode = self.read_inode(inode_number);
    inode.mode = mode;
    Ok(())
  }

  /// Replace specified inode in `free_inode_numbers` with `NO_ADDRESS`
  #[allow(dead_code)]
  fn claim_free_inode(&mut self) -> Result<AddressSize, Errno> {
    let free_inode_numbers_stub = self.superblock.free_inode_numbers.clone();
    let maybe_free_inode_numbers = free_inode_numbers_stub
      .iter()
      .find(|&&inode_number| inode_number != NO_ADDRESS);

    let free_inode_number = match maybe_free_inode_numbers {
      Some(free_inode_number) => *free_inode_number,
      None => {
        let free_inode_nums = (0..self.fs_info.inodes_count)
          .filter(|&inode_number| {
            let mut inode = self.read_inode(inode_number);
            match inode.mode.free() {
              0 => false,
              1 => true,
              _ => panic!("INode::mode::free() is not 1 or 0 (was {})", inode.mode.free())
            }
          })
        // Take one in excess so we can use it immideately 
        .take(free_inode_numbers_stub.len())
        .collect::<Vec<AddressSize>>();

        // Write rest inode_numbers to cache
        self.superblock.free_inode_numbers = free_inode_nums[0..].try_into().unwrap();

        // Return excess one
        free_inode_nums[0] 
      }
    };

    // Find 
    let free_inode_idx_in_sb = self.superblock.free_inode_numbers
      .iter()
      .zip(0..)
      .find_map(|(&inode_number, index)| {
        if inode_number == free_inode_number {
          Some(index)
        } else {
          None
        }
      }).expect("claim_free_inode: specified inode is not present in superblock.free_inode_numbers");

    // Replace inode number in superblock with NO_ADDRESS
    self.superblock.free_inode_numbers[free_inode_idx_in_sb as usize] = NO_ADDRESS;

    // Write changed superblock to disk
    self
      .write_superblock(&self.superblock.clone())
      .expect("claim_free_inode: cannot write superblock in use_inode");

    // Get inode from disk, change it to be not free
    let mut inode = self.read_inode(free_inode_number);
    inode.mode = inode.mode.with_free(0);

    // Write changed inode to disk
    self
      .write_inode(&inode, free_inode_number)
      .expect("claim_free_inode: cannot write claimed inode");
    
    Ok(free_inode_number)
  }

  /// Release specified inode
  #[allow(dead_code)]
  fn release_inode(&mut self, inode_number: AddressSize) -> Result<(), Errno> {
    // Get inode from disk, change it to be not free
    let mut inode = self.read_inode(inode_number);
    inode.mode = inode.mode.with_free(1);

    // Write changed inode to disk
    self
      .write_inode(&inode, inode_number)
      .expect("cannot write released inode");
    
    Ok(())
  }

  fn find_fbl_block<F>(&mut self, f: F) -> Result<Option<(Vec<u32>, (u32, usize))>, Errno> 
    where F: Fn(AddressSize) -> bool
  {
    let result = (self.fs_info.first_flb_block_number..self.fs_info.blocks_count)
      .map(|fbl_block_number| {
        E5FSFilesystem::parse_block_numbers_from_block(&self.read_block(fbl_block_number))
      })
      .find_map(|fbl_block| {
        let maybe_fbl_number_and_index = fbl_block
          .iter()
          .zip(0..)
          .find_map(|(&block_number, index)| { 
            match f(block_number) {
              true => Some((block_number, index)),
              false => None,
            }
          });

        match maybe_fbl_number_and_index {
          Some(fbl_number_and_index) => Some((fbl_block, fbl_number_and_index)),
          None => None
        }
      });

      Ok(result)
  }

  /// Replace specified inode in `free_inode_numbers` with `NO_ADDRESS`
  #[allow(dead_code)]
  fn claim_free_block(&mut self) -> Result<AddressSize, Errno> {
    // 1. Basically try to find first chunk with free block number != NO_ADDRESS
    let maybe_free_block_numbers = self.find_fbl_block(|block_number| block_number != NO_ADDRESS)?;

    // 2. Then see if we actually have at least one such chunk
    let free_block_numbers = match maybe_free_block_numbers {
      None => return Err(Errno::ENOENT("no fbl chunk with free block number != NO_ADDRESS (unclaimed slot)")),
      Some(free_block_numbers) => free_block_numbers,
    };
    
    let (mut fbl_chunk, (fbl_block_number, free_block_number_index)) = free_block_numbers;

    // 3. Save copy of `free_block_number` that we've found
    let free_block_number = fbl_chunk.get(free_block_number_index).unwrap().to_owned();

    // 4. Then write `NO_ADDRESS` in place of the original
    *fbl_chunk.get_mut(free_block_number_index).unwrap() = NO_ADDRESS;

    // 5. Write mutated fbl_chunk
    self.write_block(&Block {
      data: fbl_chunk.iter().flat_map(|x| x.to_le_bytes()).collect(),
    }, fbl_block_number).unwrap();

    // 5. And return it
    Ok(free_block_number)
  }

  /// Replace specified inode in `free_inode_numbers` with `NO_ADDRESS`
  /// FIXME: block_number may left dangling in inode's fields
  #[allow(dead_code)]
  fn release_block(&mut self, block_number: AddressSize) -> Result<(), Errno> {
    // 1. Basically try to find first chunk with free block number == NO_ADDRESS
    let maybe_free_block_numbers = self.find_fbl_block(|block_number| block_number == NO_ADDRESS)?;

    // 2. Then see if we actually have at least one such chunk
    let free_block_numbers = match maybe_free_block_numbers {
      None => return Err(Errno::ENOENT("no fbl chunk with free block number == NO_ADDRESS (claimed slot)")),
      Some(free_block_numbers) => free_block_numbers,
    };

    let (mut fbl_chunk, (fbl_block_number, free_block_number_index)) = free_block_numbers;

    // 3. Save copy of `free_block_number` that we've found
    let free_block_number = fbl_chunk.get(free_block_number_index).unwrap().to_owned();

    // 4. Then write block_address in place of the original `NO_ADDRESS` block
    *fbl_chunk.get_mut(free_block_number_index).unwrap() = block_number;

    // 5. Write mutated fbl_chunk
    self.write_block(&Block {
      data: fbl_chunk.iter().flat_map(|x| x.to_le_bytes()).collect(),
    }, fbl_block_number).unwrap();

    // 5. And return it
    Ok(())
  }

  /// Returns:
  /// ENOENT -> if no free block or inode exists
  fn allocate_file(&mut self) -> Result<(AddressSize, INode), Errno> {
    let free_inode_number = self.claim_free_inode()?;

    let mut inode = INode {
      mode: FileMode::default().with_free(0),
      links_count: 0,
      file_size: 0,
      uid: NOBODY,
      gid: NOBODY,
      atime: 0,
      mtime: 0,
      ctime: 0,
      number: free_inode_number,
      ..Default::default()
    };

    let free_block_number = self.claim_free_block()?;
    inode.direct_block_numbers[0] = free_block_number;

    self.write_inode(&inode, free_inode_number)?;
    self.write_block(&Block {
      data: vec![0; self.fs_info.block_data_size as usize],
    }, free_inode_number)?;

    Ok((free_inode_number, inode))
  }
  
  fn grow_file(&mut self, inode_number: AddressSize, blocks_count: AddressSize) -> Result<INode, Errno> {
    // Read inode
    let mut inode = self.read_inode(inode_number);

    // Find first empty slot
    let empty_slot = inode.direct_block_numbers
      .iter()
      .zip(0..)
      .find_map(|(&block_number, slot_index)| if block_number == NO_ADDRESS {
        Some(slot_index)
      } else {
        None
      }).ok_or_else(|| Errno::EIO("no more empty block slots in inode"))?;

    let free_slots_count = inode.direct_block_numbers.len() as AddressSize - (empty_slot + 1);

    // Guard for not enough empty slots in direct block number array
    // TODO: implement indirect blocks
    match free_slots_count {
      n if n < blocks_count => return Err(Errno::EIO("not enough empty block slots in inode")),
      _ => (),
    };

    // Allocate new blocks and store their numbers
    let block_numbers = (0..blocks_count).fold(Vec::new(), |mut block_numbers, _| {
      block_numbers.push(self.claim_free_block());
      block_numbers
    })
      .into_iter()
      .collect::<Result<Vec<AddressSize>, Errno>>()?;

    // Write these block numbees to direct blocks of inode
    block_numbers
      .iter()
      .zip(empty_slot..inode.direct_block_numbers.len() as AddressSize)
      .for_each(|(&block_number, index)| {
        inode.direct_block_numbers[index as usize] = block_number;
      });

    // Write modified inode to the disk
    self.write_inode(&inode, inode_number)?;

    Ok(inode)
  }

  fn shrink_file(&mut self, inode_number: AddressSize, blocks_count: AddressSize) -> Result<(), Errno> {
    // Read inode
    let mut inode = self.read_inode(inode_number);

    // Find last used slot
    let first_used_slot = inode.direct_block_numbers
      .iter()
      .zip((0..inode.direct_block_numbers.len()).rev())
      .find_map(|(&block_number, slot_index)| if block_number == NO_ADDRESS {
        Some(slot_index as AddressSize)
      } else {
        None
      }).ok_or_else(|| Errno::EIO("no block slots used in inode - can't shrink"))?;

    let used_slots_count = inode.direct_block_numbers.len() as AddressSize - (first_used_slot + 1);

    // Guard for not enough used slots in direct block number array
    // TODO: implement indirect blocks
    match used_slots_count {
      n if blocks_count > n => return Err(Errno::EIO("not enough used slots in inode - can't shrink")),
      _ => (),
    };

    // Release N blocks
     inode.direct_block_numbers[first_used_slot as usize..]
      .iter_mut()
      .for_each(|block_number| {
        self.release_block(*block_number).unwrap();
        *block_number = NO_ADDRESS;
      });

    // Write modified inode to the disk
    self.write_inode(&inode, inode_number)?;

    Ok(())
  }

  // Errors:
  // ENOENT -> block_number does not exist
  fn write_block(&mut self, block: &Block, block_number: AddressSize) -> Result<(), Errno> {
    // Guard for block_number out of bounds
    if block_number > self.fs_info.blocks_count {
      return Err(Errno::ENOENT("write_block: block_number out of bounds"))
    }

    // Read bytes from file
    let mut block_bytes = Vec::new();
    block_bytes.write(&block.data).unwrap();

    // Get absolute address of block
    let address = self.fs_info.first_block_address + block_number * self.fs_info.block_size;

    // Seek to it and write bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.write_all(&block_bytes).unwrap();

    Ok(())
  }
  
  #[allow(dead_code)]
  fn write_inode(&mut self, inode: &INode, inode_number: AddressSize) -> Result<(), Errno> {
    // Guard for inoe_number out of bounds
    if inode_number > self.fs_info.inodes_count {
      return Err(Errno::ENOENT("write_inode: inode_number out of bounds"))
    }
    
    // Read bytes from file
    let mut inode_bytes = Vec::new();
    inode_bytes.write(&inode.mode.0.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.links_count.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.uid.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.gid.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.file_size.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.atime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.mtime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.ctime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.direct_block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    inode_bytes.write(&inode.indirect_block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();

    // Get absolute address of inode
    let address = self.fs_info.first_inode_address + inode_number * self.fs_info.inode_size;

    // Seek to it and write bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.write_all(&inode_bytes).unwrap();

    Ok(())
  }

  #[allow(dead_code)]
  fn write_superblock(&mut self, superblock: &Superblock) -> Result<(), Errno> {
    // Read bytes from file
    let mut superblock_bytes = Vec::new();
    superblock_bytes.write(&superblock.filesystem_type).unwrap();
    superblock_bytes.write(&superblock.filesystem_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inode_table_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inode_table_percentage.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_inodes_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_blocks_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inodes_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.blocks_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.block_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.block_data_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_inode_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    superblock_bytes.write(&superblock.first_fbl_block_number.to_le_bytes()).unwrap();

    // Seek to 0 and write bytes
    self.fs_info.realfile.seek(SeekFrom::Start(0)).unwrap();
    self.fs_info.realfile.write_all(&superblock_bytes).unwrap();

    Ok(())
  }

  #[allow(dead_code)]
  fn read_block(&mut self, block_number: AddressSize) -> Block {
    let mut block_bytes = vec![0u8; self.fs_info.block_size.try_into().unwrap()];

    // Get absolute address of block
    let address = self.fs_info.first_block_address + block_number * self.fs_info.block_size;

    // Seek to it and read bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap()).try_into().unwrap()).unwrap();
    self.fs_info.realfile.read_exact(&mut block_bytes).unwrap();

    // Return bytes as is, as it is raw data of a file
    Block {
      data: block_bytes,
    }
  }

  #[allow(dead_code)]
  fn read_inode(&mut self, inode_number: AddressSize) -> INode {
    use std::mem::size_of;

    let mut inode_bytes = vec![0u8; self.fs_info.inode_size.try_into().unwrap()];

    // Get absolute address of inode
    let address = self.fs_info.first_inode_address + inode_number * self.fs_info.inode_size;

    // Seek to it and read bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.read_exact(&mut inode_bytes).unwrap();

    // Then parse bytes, draining from vector mutably
    let mode = FileMode(u16::from_le_bytes(inode_bytes.drain(0..size_of::<u16>()).as_slice().try_into().unwrap())); 
    let links_count = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); 
    let uid = Id::from_le_bytes(inode_bytes.drain(0..size_of::<Id>()).as_slice().try_into().unwrap()); 
    let gid = Id::from_le_bytes(inode_bytes.drain(0..size_of::<Id>()).as_slice().try_into().unwrap());
    let file_size = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let atime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());
    let mtime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());
    let ctime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());
    let direct_block_numbers = (0..12).fold(Vec::new(), |mut block_addresses, _| {
      block_addresses.push(AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
      block_addresses
    });
    let indirect_block_numbers = (0..3).fold(Vec::new(), |mut block_addresses, _| {
      block_addresses.push(AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
      block_addresses
    });

    // Return parsed
    INode {
      mode,
      links_count,
      uid,
      gid,
      file_size,
      atime,
      mtime,
      ctime,
      direct_block_numbers: direct_block_numbers.try_into().unwrap(),
      indirect_block_numbers: indirect_block_numbers.try_into().unwrap(),
      number: inode_number
    }
  }

  fn read_superblock(device_realpath: &str) -> Superblock {
    use std::mem::size_of;

    let mut superblock_bytes = vec![0u8; Superblock::size().try_into().unwrap()];

    let mut realfile = std::fs::OpenOptions::new()
      .read(true)
      .write(true)
      .open(device_realpath)
      .unwrap();

    realfile.seek(SeekFrom::Start(0)).unwrap();
    realfile.read_exact(&mut superblock_bytes).unwrap();

    // Then parse bytes, draining from vector mutably
    let filesystem_type: [u8; 16] = superblock_bytes.drain(0..16).as_slice().try_into().unwrap(); 
    let filesystem_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let inode_table_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let inode_table_percentage = f32::from_le_bytes(superblock_bytes.drain(0..size_of::<f32>()).as_slice().try_into().unwrap());
    let free_inodes_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let free_blocks_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let inodes_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let blocks_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let block_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let block_data_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let free_inode_numbers = (0..16).fold(Vec::new(), |mut free_inode_numbers, _| {
      free_inode_numbers.push(AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
      free_inode_numbers 
    });
    let first_fbl_block_number = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());

    Superblock {
      filesystem_type,
      filesystem_size,
      inode_table_size,
      inode_table_percentage,
      free_inodes_count,
      free_blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      block_data_size,
      free_inode_numbers: free_inode_numbers.try_into().unwrap(),
      first_fbl_block_number,
    }
  }

  /// Parse one fbl block and return it for further use
  fn parse_block_numbers_from_block(block: &Block) -> Vec<AddressSize> {
    use std::mem::size_of;
    let data = block.data.clone();

    data
      .chunks(size_of::<AddressSize>())
      .map(|chunk| AddressSize::from_le_bytes(chunk.try_into().unwrap()))
      .collect::<Vec<AddressSize>>()
  }

  /// Parse one fbl block and return it for further use
  fn parse_directory<'a>(fs_info: &E5FSFilesystemBuilder, mut data: Vec<u8>) -> Result<Directory, Errno> {
    use std::mem::size_of;

    // Read per-chunk?
    // https://github.com/torvalds/linux/blob/master/fs/ext4/dir.c#L78
    let drain_one_entry = |data: &mut Vec<u8>| -> Result<DirectoryEntry, Errno> {
      let address_size = size_of::<AddressSize>();

      let inode_number = AddressSize::from_le_bytes(data.drain(0..address_size as usize).as_slice().try_into().or_else(|_| Err(Errno::EILSEQ("can't parse inode_number")))?);
      let rec_len = u16::from_le_bytes(data.drain(0..2).as_slice().try_into().or_else(|_| Err(Errno::EILSEQ("can't parse rec_len")))?);
      let name_len = u8::from_le_bytes(data.drain(0..1).as_slice().try_into().or_else(|_| Err(Errno::EILSEQ("can't parse name_len")))?);
      let name: String = String::from_utf8(data.drain(0..name_len as usize).collect()).or_else(|_| Err(Errno::EILSEQ("can't parse name")))?;

      if inode_number >= fs_info.inodes_count {
        return Err(Errno::EILSEQ("parse_directory: drain_one_entry: inode_number out of bounds"));
      } else if (rec_len as usize) < (address_size + size_of::<u16>() + size_of::<u8>() + size_of::<u8>()) {
        return Err(Errno::EILSEQ("parse_directory: drain_one_entry: rec_len is smaller than minimal"));
      }

      Ok(DirectoryEntry {
        inode_number,
        rec_len,
        name_len,
        name,
      })
    };

    let entries_count = AddressSize::from_le_bytes(
      data.drain(0..size_of::<AddressSize>() as usize)
        .as_slice()
        .try_into()
        .or_else(|_| Err(Errno::EILSEQ("can't parse entries_count from dir")))?
      );

    let mut entries = BTreeMap::new();

    for _ in 0..entries_count {
      match drain_one_entry(&mut data) {
        Ok(entry) => { 
          //println!("parse_directory: inserting: {:?} - {:?}", entry.name.to_owned(), entry);
          entries.insert(entry.name.to_owned(), entry); 
        },
        Err(errno) => {
          //println!("parse_directory: errno: {:?}", errno);
          eprintln!("info: parse_directory: got to the end of directory: errno: {:?}", errno);
          break;
        },
      }
    }

    Ok(Directory::from(entries))
  }

  fn generate_fbl(&self) -> Vec<Vec<AddressSize>> {
    // Basically what we do in the next ~30 lines:
    // 1. Generate addresses of free blocks
    // 2. Divide it to chunks of however many fits to one block
    // 3. If addresses doesnt divide into chunks evenly,
    //    leave a remainder in separate small chunk
    // 4. If above is the case and remainder was created,
    //    fill it to be the size of full chunks
    //    with NO_BLOCK addresses
    let free_blocks_numbers = 
      (0..self.fs_info.first_flb_block_number).collect::<Vec<AddressSize>>();

    let fbl_chunks_iterator = 
      free_blocks_numbers
        .chunks_exact(self.fs_info.block_numbers_per_fbl_chunk as usize);

    let mut fbl_chunks = 
      fbl_chunks_iterator
        .clone()
        .map(|chunk| chunk.to_owned())
        .collect::<Vec<Vec<AddressSize>>>();

    let mut remainder = fbl_chunks_iterator
      .remainder()
      .to_vec();

    let lacking_addresses_count = self.fs_info.block_numbers_per_fbl_chunk - remainder.len() as AddressSize;

    if lacking_addresses_count > 0 {
      remainder
        .append(&mut vec![NO_ADDRESS; lacking_addresses_count as usize]);
    }

    fbl_chunks.push(remainder);

    fbl_chunks
  }

  #[allow(dead_code)]
  fn write_fbl(&mut self) {
    let fbl_chunks = self.generate_fbl();

    let block_numbers = self.fs_info.first_flb_block_number..self.fs_info.blocks_count;

    // Write free blocks list to last N blocks
    // [ sb ... i1..iN ... b1[b1..bX ... fbl1..fblN]bN ]
    // Something like that ^
    fbl_chunks
      .iter()
      // Zip chunks with fbl block numbers
      .zip(block_numbers)
      .for_each(|(chunk, block_number)| {
        let block = Block {
          data: chunk.iter().flat_map(|x| x.to_le_bytes()).collect(),
        };
        self.write_block(&block, block_number).unwrap();
      });
  }
  fn split_path(pathname: &str) -> Result<(Vec<String>, String), Errno> {
    // Guard for empty `pathname`
    match &pathname {
      pathname if pathname.chars().count() == 0 => return Err(Errno::EINVAL("e5fs.lookup_path: zero-length path")),
      pathname if pathname
        .chars()
        .nth(0)
        .unwrap() != '/' => return Err(Errno::EINVAL("e5fs.lookup_path: path must start with '/'")),
      _ => (),
    };

    // Replace all adjacent slashes
    let mut pathname = fixedpoint(|pathname| pathname.replace("//", "/"), pathname.to_owned());

    // Base case: return root directory '/'
    if pathname == "/" {
      // Zeroeth inode shall always be root inode
      return Ok((Vec::new(), "/".to_owned()));
    }

    // "Recurse" case: we know that path len is greater than 1 and is not equal to '/' 

    // Remove trailing slash - which must be here
    pathname.remove(0);

    // Remove ending slash if present
    if pathname
      .chars()
      .last()
      .expect("we know that chars().count() >= 1 but is not '/' 
              because of guard and base case") == '/' 
    {
      pathname.pop();
    }
    
    // let everything_else: Vec<String> = pathname
    //   .split('/')
    //
    //   // First (root) piece is "" - make it "/"
    //   .enumerate()
    //   .map(|(index, piece)| if index == 0 { "/" } else { piece })
    //
    //   // Take one less (drop final_component)
    //   .take(pathname.split('/').count() - 1)
    //
    //   // Map to owned (make borrow checker happy)
    //   .map(|piece| piece.to_owned())
    //   
    //   // Collect to vector
    //   .collect();
    let everything_else: Vec<String> = pathname
      .split('/')
      .take(pathname.split('/').count() - 1)
      .map(|piece| piece.to_owned())
      .collect();
    let final_component: String = pathname
      .split('/')
      .last()
      .expect("e5fs.split_path: we know that there is element").to_owned();

    match pathname.split('/').count() {
      // E.g. with '/test1' we have vec!["", "test1"]
      1 => Ok((Vec::new(), final_component)),
      _ => Ok((everything_else, final_component)),
    }
  }
}

#[cfg(test)]
mod e5fs_fs_tests {
  use std::array::IntoIter;

use crate::{util::{mktemp, mkenxvd}, eunix::fs::NOBODY};
  use super::*;

  #[test]
  fn write_superblock_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let superblock = Superblock::new(&mut e5fs.fs_info);
    e5fs.write_superblock(&superblock).unwrap();

    drop(e5fs);

    let superblock_from_file = E5FSFilesystem::read_superblock(tempfile.as_str());

    assert_eq!(superblock_from_file, superblock);
  }

  #[test]
  fn write_inode_works() {
    // let tempfile = "/tmp/tmp.4yOs4cciU1".to_owned();
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let inode_indices = 0..e5fs.fs_info.blocks_count;
    let inodes = inode_indices.clone().fold(Vec::new(), |mut vec, i| {
      vec.push(INode {
        mode: FileMode::zero() + FileMode::new(1),
        links_count: i,
        uid: (i as Id % NOBODY),
        gid: (i as Id % NOBODY) + 1,
        file_size: i * 1024,
        atime: crate::util::unixtime(),
        mtime: crate::util::unixtime(),
        ctime: crate::util::unixtime(),
        direct_block_numbers: [i % 5; 12],
        indirect_block_numbers: [i % 6; 3],
        number: i,
      });

      vec
    });

    inodes
      .iter()
      .zip(inode_indices.clone())
      .for_each(|(inode, inode_number)| {
        e5fs.write_inode(inode, inode_number).unwrap();
      });

    inodes
      .iter()
      .zip(inode_indices.clone())
      .for_each(|(inode, inode_number)| {
        let inode_from_file = e5fs.read_inode(inode_number);

        assert_eq!(*inode, inode_from_file);
      });
  }

  #[test]
  fn write_block_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let block_indices = 0..e5fs.fs_info.blocks_count;
    let blocks = block_indices.clone().fold(Vec::new(), |mut vec, i| {
      vec.push(Block {
        data: vec![(i % 255) as u8; e5fs.fs_info.block_data_size as usize],
      });

      vec
    });

    blocks
      .iter()
      .zip(block_indices.clone())
      .for_each(|(block, block_number)| {
        e5fs.write_block(block, block_number).unwrap();
      });

    blocks
      .iter()
      .zip(block_indices.clone())
      .for_each(|(block, block_number)| {
        let block_from_file = e5fs.read_block(block_number);

        assert_eq!(*block, block_from_file);
      });
  }

  #[test]
  fn write_fbl_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    e5fs.write_fbl();

    let fbl_chunks = e5fs.generate_fbl();

    let fbl_chunks_from_file = (e5fs.fs_info.first_flb_block_number..e5fs.fs_info.blocks_count)
      .map(|block_number| {
        let block = e5fs.read_block(block_number);
        let block_numbers_from_block = E5FSFilesystem::parse_block_numbers_from_block(&block);

        block_numbers_from_block
      })
      .collect::<Vec<Vec<AddressSize>>>();

    assert_eq!(fbl_chunks_from_file, fbl_chunks);
  }

  #[test]
  fn read_block_numbers_from_block_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let block_numbers_per_free_blocks_chunk = e5fs.fs_info.block_data_size / std::mem::size_of::<AddressSize>() as AddressSize;

    let block_numbers: Vec<AddressSize> = (0..block_numbers_per_free_blocks_chunk).collect();

    let block = Block {
      data: block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect(),
    };

    let block_numbers_from_block = E5FSFilesystem::parse_block_numbers_from_block(&block);

    assert_eq!(block_numbers_from_block, block_numbers);
  }
  
  #[test]
  fn allocate_file_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    // Create 2 files
    let (file1_inode_num, file1_inode) = e5fs.allocate_file().unwrap();
    let (file2_inode_num, file2_inode) = e5fs.allocate_file().unwrap();

    assert_eq!(file1_inode_num, 1, "file 1 inode should be of num 1 (first is root_inode)");
    assert_eq!(file2_inode_num, 2, "file 2 inode should be of num 2 (first is root_inode)");

    let file1_inode_from_disk = e5fs.read_inode(1);
    let file2_inode_from_disk = e5fs.read_inode(2);

    assert_eq!(file1_inode_from_disk, file1_inode);
    assert_eq!(file2_inode_from_disk, file2_inode);
  }

  #[test]
  fn write_and_read_directory_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    // Create 2 files
    let (file1_inode_num, _) = e5fs.allocate_file().unwrap();
    e5fs.write_to_file("hello world1".as_bytes().to_owned(), file1_inode_num, false).unwrap();
    let (file2_inode_num, _) = e5fs.allocate_file().unwrap();
    e5fs.write_to_file("hello world2".as_bytes().to_owned(), file2_inode_num, false).unwrap();

    let root_inode_number = e5fs.fs_info.root_inode_number;
    let expected_dir = Directory {
      entries_count: 2,
      entries: BTreeMap::from_iter(IntoIter::new([
        (String::from("hello-world1.txt"), DirectoryEntry::new(file1_inode_num, "hello-world1.txt").unwrap()),
        (String::from("hello-world2.txt"), DirectoryEntry::new(file2_inode_num, "hello-world2.txt").unwrap()),
      ])),
    };
    e5fs.write_dir(&expected_dir, root_inode_number).unwrap();

    let dir_from_disk = e5fs.read_dir_from_inode(root_inode_number).unwrap();

    assert_eq!(dir_from_disk, expected_dir);
  }

  #[test]
  fn find_flb_block_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let (fbl_block, (free_block_num, free_block_num_idx)) = e5fs.find_fbl_block(|block_number| block_number != NO_ADDRESS).unwrap().unwrap();

    // Simulate fbl with rest left as NO_ADDRESS
    let expected = (0..e5fs.fs_info.first_flb_block_number)
      .chain(
        (e5fs.fs_info.first_flb_block_number..e5fs.fs_info.block_numbers_per_fbl_chunk).map(|_| NO_ADDRESS)
      )
      .collect::<Vec<u32>>();

    assert_eq!(fbl_block, expected);
  }

  #[test]
  fn lookup_path_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("10M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    //println!("{}", e5fs.fs_info.inodes_count);

    let root_vinode = e5fs.lookup_path("/").unwrap();
    let root_inode = e5fs.read_inode(0);

    assert_eq!(root_vinode, root_inode.into());

    // Create 10 files
    let first_layer_files = (0..10 as AddressSize).fold(Vec::new(), |mut acc, cur| {
      let (inode_num, inode) = e5fs.allocate_file().unwrap();
      e5fs.write_to_file(cur.to_string().as_bytes().to_owned(), inode_num, false).unwrap();
      acc.push((inode_num, inode));
      acc
    });

    // Create 100 files to write 10 to each of 10 previous
    let second_layer_files = (0..100 as AddressSize).fold(Vec::new(), |mut acc, cur| {
      let (inode_num, inode) = e5fs.allocate_file().unwrap();
      let filename = format!("{}.txt", cur.to_string());
      e5fs.write_to_file(filename.as_bytes().to_owned(), inode_num, false).unwrap();
      acc.push((inode_num, inode));
      acc
    });

    let root_inode_number = e5fs.fs_info.root_inode_number;
    let root_dir = Directory {
      entries_count: first_layer_files.len() as AddressSize,
      entries: first_layer_files
        .iter()
        .enumerate()
        .map(|(index, (inode_num, _inode))| {
          //println!("index_exp: {}", index);
          let name = index.to_string().as_str().to_owned();
          (name.to_owned(), DirectoryEntry::new(*inode_num, &name).unwrap())
        })
        .collect(), 
    };
    e5fs.write_dir(&root_dir, root_inode_number).unwrap();

    //println!("root_dir: {:#?}", root_dir);
    first_layer_files
      .iter()
      .zip(second_layer_files.chunks(10))
      .enumerate()
      .for_each(|(outer_index, ((outer_f_inode_num, _outer_f_inode), inner_files))| {
        let dir = Directory {
          entries_count: 10,
          entries: inner_files
            .iter()
            .enumerate()
            .map(|(index, (inode_num, _inode))| {
              let name = index.to_string();
              (name.to_owned(), DirectoryEntry::new(*inode_num, &name).unwrap())
            })
            .collect(), 
        };
        e5fs.write_dir(&dir, *outer_f_inode_num).unwrap();

        inner_files
          .iter()
          .enumerate()
          .for_each(|(inner_index, (inner_f_inode_num, _innder_f_inode))| {
            let file_contents = format!("{}-{}", outer_index, inner_index);
            e5fs.write_to_file(file_contents.as_bytes().to_owned(), *inner_f_inode_num, false).unwrap();
          });
      });

    //println!("first_layer_files: {:?}", first_layer_files);

    let first_layer_files_from_disk = (0..10).fold(Vec::new(), |mut files, cur| {
      let vinode = e5fs.lookup_path(&format!("/{}", cur.to_string().as_str())).unwrap();
      files.push(vinode.number);
      files
    });

    assert_eq!(first_layer_files_from_disk, first_layer_files.iter().map(|(inode_num, _)| *inode_num).collect::<Vec<AddressSize>>());
  }

  #[test]
  fn create_file_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let vinode = e5fs.create_file("/test1").unwrap();
    let vinode_from_disk: VINode = e5fs.read_inode(1).into();

    assert_eq!(vinode_from_disk, vinode);
  }
  
  #[test]
  fn create_nested_file_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let vinode1 = e5fs.create_file("/test12").unwrap();

    // Change type to Dir
    let mut inode1 = e5fs.read_inode(vinode1.number);
    //println!("inode1_mode b: {:016b}", inode1.mode.0);
    inode1.mode = inode1.mode.with_type(FileModeType::Dir as u8);
    //println!("inode1_mode a: {:016b}", inode1.mode.0);
    e5fs.write_inode(&inode1, inode1.number).unwrap();

    let vinode2 = e5fs.create_file("/test12/test2").unwrap();

    assert_eq!(vinode2.number, 2);
  }
}

#[cfg(test)]
mod e5fs_split_path_tests {
  use super::*;

  #[test]
  fn split_path_root() {
    assert_eq!(E5FSFilesystem::split_path("/").unwrap(), ((Vec::new(), String::from("/"))));
  }
  #[test]
  fn split_path_only_slashes() {
    assert_eq!(E5FSFilesystem::split_path("//////").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(E5FSFilesystem::split_path("/////").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(E5FSFilesystem::split_path("////").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(E5FSFilesystem::split_path("///").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(E5FSFilesystem::split_path("//").unwrap(), ((Vec::new(), String::from("/"))));
  }
  #[test]
  fn split_path_valid_1() {
    assert_eq!(E5FSFilesystem::split_path("/test1").unwrap(), ((Vec::new(), String::from("test1"))));
  }
  #[test]
  fn split_path_valid_2() {
    assert_eq!(E5FSFilesystem::split_path("/test1/test2").unwrap(), ((vec![String::from("test1")], String::from("test2"))));
  }
  #[test]
  fn split_path_valid_3() {
    assert_eq!(E5FSFilesystem::split_path("/test1/test2/test3").unwrap(), ((vec![String::from("test1"), String::from("test2")], String::from("test3"))));
  }
  #[test]
  fn split_path_valid_multiple_slashes() {
    assert_eq!(E5FSFilesystem::split_path("//test1//test2///test3////").unwrap(), ((vec![String::from("test1"), String::from("test2")], String::from("test3"))));
  }
  #[test]
  fn split_path_valid_onechar_1() {
    assert_eq!(E5FSFilesystem::split_path("/a").unwrap(), ((Vec::new(), String::from("a"))));
  }
  #[test]
  fn split_path_valid_onechar_2() {
    assert_eq!(E5FSFilesystem::split_path("/a/b").unwrap(), ((vec![String::from("a")], String::from("b"))));
  }
  #[test]
  fn split_path_valid_onechar_3() {
    assert_eq!(E5FSFilesystem::split_path("/a/b/c").unwrap(), ((vec![String::from("a"), String::from("b")], String::from("c"))));
  }
  #[test]
  fn split_path_zero_length() {
    match E5FSFilesystem::split_path("") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL("e5fs.lookup_path: zero-length path")),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_1() {
    match E5FSFilesystem::split_path("test1") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL("e5fs.lookup_path: path must start with '/'")),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_1_trailing() {
    match E5FSFilesystem::split_path("test1/") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL("e5fs.lookup_path: path must start with '/'")),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_2() {
    match E5FSFilesystem::split_path("test1/test2") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL("e5fs.lookup_path: path must start with '/'")),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_3() {
    match E5FSFilesystem::split_path("test1/test2/test3") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL("e5fs.lookup_path: path must start with '/'")),
      _ => unreachable!(),
    };
  }
}

// vim:ts=2 sw=2
