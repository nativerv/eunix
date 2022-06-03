use core::fmt;
use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Write;
use std::slice::SliceIndex;

use fancy_regex::Regex;
use itertools::Itertools;

use crate::eunix::fs::FileModeType;
use crate::eunix::fs::NOBODY;
use crate::eunix::kernel::UnixtimeSize;
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
use super::fs::VFS;
use super::kernel::Errno;
use super::kernel::Times;

struct FindFblBlockResult {
  fbl_block_number: AddressSize,
  index_in_fbl_block: usize,
  fbl_chunk: Vec<AddressSize>,
}

/* 
 * LEGEND: 
 * fbl       - free blocks list, the reserved blocks at the
 *             end of the blocks list which contain free
 *             block numbers for quick allocation
 * fbl_chunk - vector of numbers parsed from `fbl` block
 * fbl_index - index into `fbl` by step of address_size
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
      name_len: name.len().try_into().or_else(|_| Err(Errno::ENAMETOOLONG(String::from("DirectoryEntry::new: name can't be bigger than 255"))))?,
      name: name.to_owned(),
    })
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Directory {
  entries_count: AddressSize,
  pub entries: BTreeMap<String, DirectoryEntry>,
}

impl fmt::Display for Directory {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Directory:")?;
    for (_, DirectoryEntry { inode_number, name, .. }) in &self.entries {
      writeln!(f, "  {name}\t{inode_number}")?;
    }
    Ok(())
  }
}

impl Directory {
  pub fn new() -> Self {
    Self {
      entries_count: 0,
      entries: BTreeMap::new(),
    }
  }
  fn from(entries: BTreeMap<String, DirectoryEntry>) -> Self {
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
    // Guard for entry already existing
    if let Some(_) = self.entries.get(name) {
      return Err(Errno::EEXIST(format!("e5fs::Directory: entry {name} already exists")))
    }
    self.entries.insert(
      name.to_owned(),
      DirectoryEntry::new(inode_number, name)?
    );
    self.entries_count += 1;
    Ok(())
  }
  pub fn remove(&mut self, name: &str) -> Result<(), Errno> {
    self.entries.remove(name).ok_or(Errno::ENOENT(String::from("no such file in directory")))?;
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
  atime: UnixtimeSize,
  mtime: UnixtimeSize,
  ctime: UnixtimeSize,
  btime: UnixtimeSize,
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
      btime: inode.btime,
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
impl From<VDirectoryEntry> for DirectoryEntry {
  fn from(entry: VDirectoryEntry) -> Self {
    DirectoryEntry::new(entry.inode_number, &entry.name).unwrap()
  }
}

impl From<Directory> for VDirectory {
  fn from(dir: Directory) -> Self {
    Self {
      entries: dir.entries.into_iter().map(|(key, entry)| (key, entry.into())).collect(),
    }
  }
}
impl From<VDirectory> for Directory {
  fn from(dir: VDirectory) -> Self {
    Self {
      entries_count: dir.entries.len() as AddressSize,
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
      btime: 0,
      direct_block_numbers: [NO_ADDRESS; 12],
      indirect_block_numbers: [NO_ADDRESS; 3],
      number: 0,
    }
  }
}

#[derive(Default, Debug, PartialEq, Clone, Copy)]
pub struct Superblock {
  /// A name of filesystem, basically
  pub filesystem_type: [u8; 16],
  /// In blocks? Dc, unused anyway
  pub filesystem_size: AddressSize, 
  /// Total inode table size in bytes
  pub inode_table_size: AddressSize,
  /// Percentage of inode table in relation to total disk size
  pub inode_table_percentage: f32,
  /// Count of free inodes
  pub free_inodes_count: AddressSize,
  /// Count of free blocks
  pub free_blocks_count: AddressSize,
  /// Count of inodes on the filesystem
  pub inodes_count: AddressSize,
  /// Count of blocks on the filesystem
  pub blocks_count: AddressSize,
  /// Size of a single block (in bytes)
  /// Should be equal to `block_size` 
  /// (earlier blocks contained info besides
  /// actual data)
  pub block_size: AddressSize,
  /// Size of data on a single block (in bytes)
  pub block_data_size: AddressSize,
  /// Cache of free inode numbers - gets replenished automatically
  pub free_inode_numbers: [AddressSize; 16],
  /// Block number of first `free block list` block -
  /// a list of blocks containing free block numbers as
  /// contents
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

    // Sanity check
    assert_eq!(block_size, block_data_size, "`block_size` should equal `block_data_size`");

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
pub struct E5FSFilesystemBuilder {
  realfile: RefCell<std::fs::File>,
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
  first_fbl_block_number: AddressSize,
  first_fbl_block_address: AddressSize,
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

    let mut realfile = RefCell::new(std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(device_realpath)
                    .unwrap());

    let device_size = realfile.borrow_mut().metadata().unwrap().len() as AddressSize;
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
    let blocks_needed_for_fbl = 
      (blocks_count as f64 / (block_data_size as f64 / address_size as f64))
        .ceil() as AddressSize;

    // Sanity check
    if blocks_needed_for_fbl < 1 {
      return Err("blocks_needed_for_fbl can't be less than 1");
    }

    let free_blocks_count = blocks_count - blocks_needed_for_fbl;

    let block_numbers_per_fbl_chunk = block_data_size / address_size;

    // Guard for not enough blocks even for free blocks list
    if blocks_needed_for_fbl >= blocks_count {
      return Err("disk size is too small: blocks_needed_for_fbl > blocks_count");
    }

    let block_table_size = block_size * blocks_count;

    // Basically step over all free block numbers - 
    // first after that will be beginning of `fbl`
    let first_fbl_block_number = free_blocks_count;
    let first_fbl_block_address = 
      superblock_size 
      + inode_table_size 
      + (blocks_count - blocks_needed_for_fbl) * block_size;

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
      block_numbers_per_fbl_chunk,
      inode_table_percentage,
      first_fbl_block_number,
      first_fbl_block_address,
      root_inode_number: 0,
    })
  }
}

pub struct E5FSFilesystem {
  superblock: Superblock,
  fs_info: E5FSFilesystemBuilder,
}

impl Filesystem for E5FSFilesystem {
  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let (_, final_component) = VFS::split_path(pathname)?;
    let parent_pathname = VFS::parent_dir(pathname)?;

    // Get dir path with this regex
    let parent_inode = self.lookup_path(parent_pathname.as_str())?;

    // Read dir from disk
    let mut parent_dir = self.read_as_dir_i(parent_inode.number)?;

    // Guard for file already existing
    if let Some(_) = parent_dir.entries.get(&final_component)
    {
      return Err(Errno::EINVAL(format!("e5fs::create_file: file {final_component} already exists in {parent_pathname}")));
    }

    // Allocate inode
    let (_, inode) = self.allocate_file()?;

    // Push allocated to dir
    parent_dir.insert(inode.number, final_component.as_str())?;

    // Write dir
    self.write_dir_i(&parent_dir, parent_inode.number)?;

    // Set inode's links count to 1 (link from parent dir)
    self.write_links_count_i(inode.number, 1)?;

    // Read new inode before returning, just to be sure
    // that we got correct sizes and all that crap
    //
    let inode = self.read_inode(inode.number);
    Ok(inode.into())
  }

  fn remove_file(&mut self, pathname: &str)
    -> Result<(), Errno> {
    let parent_pathname = VFS::parent_dir(pathname)?;
    let (_, final_component) = VFS::split_path(pathname)?;
    let parent_vinode = self.lookup_path(&parent_pathname)?;
    let mut parent_dir = self.read_dir(&parent_pathname)?;

    if final_component == "." || final_component == ".." {
      return Err(Errno::EINVAL(format!("e5fs::remove_file: you cannot remove self or parent-reference")))
    }
    
    // Mutate dir and write (save) it
    let VDirectoryEntry {
        inode_number,
        ..
    } = parent_dir
      .entries
      .remove(&final_component)
      .ok_or(Errno::ENOENT(format!("e5fs::remove_file: no such file or directory '{final_component}'")))?;
    self.write_dir_i(&parent_dir.into(), parent_vinode.number)?;

    // Read inode and update it's values
    let mut inode = self.read_inode(inode_number);
    inode.links_count -= 1;
    inode.ctime = unixtime();

    // Free blocks of inode if no links left
    if inode.links_count < 1 {
      for block_number in self
        .iter_blocks_i(inode_number)
        .take_while(|&block_number| block_number != NO_ADDRESS)
      {
        self.release_block(block_number)?;
      }
      inode.mode = inode.mode.with_free(1);
    }

    // Write (save) inode to disk
    self.write_inode(&inode, inode.number)
  } 

  fn create_dir(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let vinode = self.create_file(pathname)?;

    let parent_pathname = format!("/{}", VFS::split_path(pathname)?.0.join("/"));
    let parent_vinode = self.lookup_path(&parent_pathname)?;

    // Construct dir with parent- and self- references
    let mut dir = Directory::new();
    dir.insert(parent_vinode.number, "..")?;
    dir.insert(vinode.number, ".")?;
    self.write_dir_i(&dir, vinode.number)?;

    // Change inode mode to be of type `Dir`
    self.write_mode_i(vinode.number, vinode.mode.with_file_type(FileModeType::Dir as u8))?;

    // Set links count of new inode
    // to 2 (self-reference and reference by a parent dir)
    self.write_links_count_i(vinode.number, 2)?;

    // Increment link count of parent inode
    self.write_links_count_i(parent_vinode.number, parent_vinode.links_count + 1)?;

    Ok(vinode)
  }

  fn read_file(&mut self, pathname: &str, _count: AddressSize)
    -> Result<Vec<u8>, Errno> {
    let vinode = self.lookup_path(pathname)?;
    if vinode.mode.file_type() == FileModeType::Dir as u8 {
      Err(Errno::EISDIR(format!("read_file: {pathname}: is a directory")))
    } else {
      self.read_data_i(vinode.number)
    }
  } 

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<VINode, Errno> {
    let vinode = self.lookup_path(pathname)?;
    if vinode.mode.file_type() == FileModeType::Dir as u8 {
      return Err(Errno::EISDIR(format!("e5fs::write_file: is a directory")))
    }
    let new_vinode: VINode = self.write_data_i(data.to_owned(), vinode.number, false)?.into();
    Ok(new_vinode)
  }

  fn read_dir(&self, pathname: &str)
    -> Result<VDirectory, Errno> {
    // // Guard for file_type = directory
    // match vinode
    //   .mode
    //   .file_type()
    //   .try_into()
    //   .unwrap()
    // {
    //   FileModeType::Dir => {
    //     Ok(self.read_as_dir_i(vinode.number)?.into())
    //   },
    //   _ => {
    //     Err(Errno::ENOTDIR(format!("e5fs::read_dir: not a directory: {pathname}")))
    //   }
    // }
    //
    let inode_number = self.lookup_path(pathname)?.number;
    let dir = self.read_as_dir_i(inode_number)?;

    Ok(dir.into())
  }

  fn stat(&self, pathname: &str) 
    -> Result<FileStat, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let INode {
      mode,
      file_size,
      links_count,
      uid,
      gid,
      atime,
      mtime,
      ctime,
      btime,
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
      atime,
      mtime,
      ctime,
      btime,
    })
  }

  fn change_mode(&mut self, pathname: &str, mode: FileMode)
    -> Result<(), Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    self.write_mode_i(inode_number, mode)
  } 

  fn change_times(&mut self, pathname: &str, times: Times)
    -> Result<(), Errno> {
    let mut inode = self.read_inode(self.lookup_path(pathname)?.number);
    inode.atime = times.atime;
    inode.mtime = times.mtime;
    inode.ctime = times.ctime;
    inode.btime = times.btime;
    self.write_inode(&inode, inode.number)
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str)
    -> Result<VINode, Errno> {
    let split_pathname = VFS::split_path(pathname)?;

    // Base case: 
    //   lookup_path /
    if split_pathname == (Vec::new(), String::from("/")) {
      let inode = self.read_inode(self.fs_info.root_inode_number);
      return Ok(inode.into());
    };

    // General case: 
    //   lookup_path /foo
    //   lookup_path /foo/bar
    //   lookup_path /foo/bar/baz
    // For every `component` in `everything_else` look for that
    // `component` inside `inode` (initially root inode),
    // replacing it with inode pointed by component
    // At the end we will have the dir which contains our
    // `final_component` (or we will do nothing, in which case the
    // dir is root inode)
    let (everything_else, final_component) = split_pathname.clone();
    let mut inode_number = self.fs_info.root_inode_number;

    for component in everything_else {
      let dir = self.read_as_dir_i(inode_number)?;
      inode_number = dir.entries
        .get(&component)
        .map(|entry| entry.inode_number)
        .ok_or(Errno::ENOENT(format!("e5fs.lookup_path: no such component: {component}")))?;
    }

    // After we advanced our inode_number for every 
    // `component` in `everything_else`, read that last
    // dir and read `final_component`'s inode from it
    let dir = self.read_as_dir_i(inode_number)?;
    dir.entries
      .get(&final_component)
      .map(|entry| self.read_inode(entry.inode_number).into())
      .ok_or(Errno::ENOENT(format!("e5fs.lookup_path: no such file or directory {final_component} (get(final_component))")))
  }

  fn name(&self) -> String { 
    String::from("e5fs")
  }

fn as_any(&mut self) -> &mut dyn Any {
      self
    } 
}

impl E5FSFilesystem {
  /// Read filesystem from device (file on host) path
  pub fn from(device_realpath: &str) -> Result<Self, Errno> {
    let superblock = E5FSFilesystem::read_superblock(device_realpath);

    let fs_info = 
      E5FSFilesystemBuilder::new(
        device_realpath, 
        superblock.inode_table_percentage, 
        superblock.block_data_size,
      )
      .unwrap();

    Ok(Self {
      superblock,
      fs_info,
    })
  }

  /// Create new filesystem and write it to disk
  pub fn mkfs(device_realpath: &str, inode_table_percentage: f32, block_data_size: AddressSize) -> Result<Self, Errno> {
    let mut fs_info = E5FSFilesystemBuilder::new(
        device_realpath, 
        inode_table_percentage, 
        block_data_size,
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
    let mut root_dir = Directory::new();
    root_dir.insert(root_inode_number, "..").expect("this should succeed");
    root_dir.insert(root_inode_number, ".").expect("this should succeed");
    e5fs.write_dir_i(&root_dir, root_inode_number)?;

    // Set mode, time, link count, gid and uid to root inode
    let mut root_inode = e5fs.read_inode(root_inode_number);
    root_inode.mode = FileMode::zero()
      .with_free(0)
      .with_file_type(FileModeType::Dir as u8)
      .with_user(0o7)
      .with_group(0o5)
      .with_others(0o5);
    root_inode.atime = unixtime();
    root_inode.mtime = unixtime();
    root_inode.ctime = unixtime();
    root_inode.btime = unixtime();
    root_inode.links_count = 2;
    root_inode.uid = 0;
    root_inode.gid = 0;
    e5fs.write_inode(&root_inode, root_inode_number)?;

    Ok(e5fs)
  }

  fn write_dir_i(&mut self, dir: &Directory, inode_number: AddressSize) -> Result<INode, Errno> {
    // We know that we're getting wrong dir data at this point already
    // Convert `Directory` to bytes
    let entries_count_bytes = dir.entries_count.to_le_bytes().as_slice().to_owned();
    let entries_bytes = dir.entries.iter().fold(Vec::new(), |mut bytes, (_name, entry)| {
      bytes.write(entry.inode_number.to_le_bytes().as_slice()).unwrap();
      bytes.write(entry.rec_len.to_le_bytes().as_slice()).unwrap();
      bytes.write(entry.name_len.to_le_bytes().as_slice()).unwrap();
      bytes.write(entry.name.as_bytes()).unwrap();
      bytes
    });

    // Write them to one `Vec`
    let mut data = Vec::new();
    data.write(&entries_count_bytes)
      .or(Err(Errno::EIO(format!("write_dir: can't write entries_count_bytes to data"))))?;
    data.write(&entries_bytes)
      .or(Err(Errno::EIO(format!("write_dir: can't write entries_bytes to data"))))?;

    // Write `Vec` to file
    let new_inode = self.write_data_i(data, inode_number, false)?;

    // NOTICE: Set inode mode to be directory (???)
    //new_inode.mode = new_inode.mode.with_type(FileModeType::Dir as u8);
    //self.write_inode(&new_inode, inode_number)?;

    Ok(new_inode)
  }

  fn read_as_dir_i(&self, inode_number: AddressSize) -> Result<Directory, Errno> {
    let dir_bytes = self.read_data_i(inode_number)?;
    let directory = E5FSFilesystem::parse_directory(&self.fs_info, dir_bytes)?;

    Ok(directory)
  }

  fn write_data_i(&mut self, data: Vec<u8>, inode_number: AddressSize, _append: bool) -> Result<INode, Errno> {
    let inode = self.read_inode(inode_number);

    // If data is greater than available in inode's blocks,
    // grow the file
    let difference = data.len() as isize - (self.get_inode_blocks_count(inode_number)? * self.fs_info.block_size) as isize;
    if difference > 0 {
      self.grow_file(inode_number, (difference as f64 / self.fs_info.block_size as f64).ceil() as AddressSize)?;
    }

    // Refresh inode from disk
    let inode = self.read_inode(inode_number);

    // Split data to chunks...
    let chunks = data
      .chunks(self.fs_info.block_size as usize)
      .zip(0..);
    // ...and write it to inode's blocks
    for (chunk, i) in chunks {
      self.write_block(&Block { data: chunk.to_owned(), }, inode.direct_block_numbers[i])?;
    };

    // Write new size to inode, and update times
    let mut inode_cloned = inode.clone();
    inode_cloned.file_size = data.len() as AddressSize;
    inode_cloned.atime = unixtime();
    inode_cloned.mtime = unixtime();
    inode_cloned.ctime = unixtime();
    self.write_inode(&inode_cloned, inode_number)?;

    Ok(inode_cloned)
  }

  fn read_data_i(&self, inode_number: AddressSize) -> Result<Vec<u8>, Errno> {
    let inode = self.read_inode(inode_number);

    let data = self
      .iter_blocks_i(inode_number)
      .take_while(|&block_number| block_number != NO_ADDRESS)
      .flat_map(|block_number| self.read_block(block_number).data)
      .take(inode.file_size as usize)
      .collect();

    Ok(data)
  }

  fn get_inode_blocks_count(&mut self, inode_number: AddressSize) -> Result<AddressSize, Errno> {
    let inode = self.read_inode(inode_number);

    Ok(
      inode
        .direct_block_numbers
        .iter()
        .take_while(|&&block_number| block_number != NO_ADDRESS)
        .map(|_| 1)
        .sum()
    )
  }

  fn read_mode(&mut self, inode_number: AddressSize) -> Result<FileMode, Errno> {
    let inode = self.read_inode(inode_number);
    Ok(inode.mode)
  }

  fn write_mode_i(&mut self, inode_number: AddressSize, mode: FileMode) -> Result<(), Errno> {
    let mut inode = self.read_inode(inode_number);
    inode.mode = mode;
    self.write_inode(&inode, inode_number)
  }

  /// Replace specified inode in `free_inode_numbers` with `NO_ADDRESS`
  fn claim_free_inode(&mut self) -> Result<AddressSize, Errno> {
    let (index, inode_number) = self
      .superblock
      .free_inode_numbers
      .clone()
      .iter()
      .enumerate()
      .find(|(_, inode_number)| **inode_number != NO_ADDRESS)
      .map(|(index, inode_number)| (index, *inode_number))
      .ok_or(Errno::ENOSPC(format!("no free inodes left (in cache, todo: fix me)")))?;

    // Replace and write inode number in superblock with NO_ADDRESS
    *self
      .superblock
      .free_inode_numbers
      .get_mut(index)
      .ok_or(
        Errno::EIO(format!("e5fs::claim_free_inode: cannot index free_inode_numbers sith {index}: this should not happen"))
      )? = NO_ADDRESS;
    self.write_superblock(&self.superblock.clone())?;

    // Write mode to not free
    let mut inode = self.read_inode(inode_number);
    inode.mode = inode.mode.with_free(0);
    self.write_inode(&inode, inode_number)?;

    Ok(inode_number)
  }

  /// Release specified inode
  fn release_inode(&mut self, inode_number: AddressSize) -> Result<(), Errno> {
    // Get inode from disk, change it to be not free
    let mut inode = self.read_inode(inode_number);
    inode.mode = inode.mode.with_free(1);

    // Write changed inode to disk
    self.write_inode(&inode, inode_number)
  }

  /// Returns block number, which is also an index into `fbl`.
  fn find_block_in_fbl<F>(&mut self, f: F) -> Result<AddressSize, Errno> 
    where F: Fn(AddressSize) -> bool
  {
    (self.fs_info.first_fbl_block_number..self.fs_info.blocks_count)
      .flat_map(|fbl_block_number| { 
        E5FSFilesystem::parse_block_numbers_from_block(
          &self.read_block(fbl_block_number)
        ) 
      })
      .find(|block_number| f(*block_number))
      .ok_or(Errno::ENOSPC(format!("e5fs::find_block_in_fbl: not found")))
  }

  /// Replace specified inode in `free_inode_numbers` with `NO_ADDRESS`
  fn claim_free_block(&mut self) -> Result<AddressSize, Errno> {
    // 1. Basically try to find index of block with number != NO_ADDRESS in `fbl`
    let block_number = self.find_block_in_fbl(|n| n != NO_ADDRESS)?;

    let address_size = self.fs_info.address_size;
    let address = self.fs_info.first_fbl_block_address + (block_number * address_size);

    // 2. Write (save to disk) NO_ADDRESS to that index
    // to indicate that this block was claimed
    self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.borrow_mut().write_all(&NO_ADDRESS.to_le_bytes()).unwrap();

    // 3. Return block_number
    Ok(block_number)
  }

  /// Replace specified inode in `fbl` with `block_number`
  /// FIXME: block_number may left dangling in inode's fields
  fn release_block(&mut self, block_number: AddressSize) -> Result<(), Errno> {
    let address_size = self.fs_info.address_size;
    let address = self.fs_info.first_fbl_block_address + (block_number * address_size);

    // 1. Write (save to disk) `block_number` to fbl index of
    // `block_number` (fbl indices correlate 1:1 to block numbers)
    // to indicate that this block is claimed
    self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.borrow_mut().write_all(&block_number.to_le_bytes()).unwrap();

    // 2. And return it
    Ok(())
  }

  /// Returns:
  /// ENOENT -> if no free block or inode exists
  fn allocate_file(&mut self) -> Result<(AddressSize, INode), Errno> {
    let inode_number = self.claim_free_inode()?;

    let mut inode = INode {
      mode: FileMode::default().with_free(0),
      links_count: 0,
      file_size: 0,
      uid: NOBODY,
      gid: NOBODY,
      atime: unixtime(),
      mtime: unixtime(),
      ctime: unixtime(),
      btime: unixtime(),
      number: inode_number,
      ..Default::default()
    };

    let block_number = self.claim_free_block()?;
    inode.direct_block_numbers[0] = block_number;

    self.write_inode(&inode, inode_number)?;
    self.write_block(&Block {
      data: vec![0; self.fs_info.block_data_size as usize],
    }, inode_number)?;

    Ok((inode_number, inode))
  }
  
  fn grow_file(&mut self, inode_number: AddressSize, blocks_count: AddressSize) -> Result<INode, Errno> {
    // Read inode
    let mut inode = self.read_inode(inode_number);

    // Find first empty slot
    let empty_slot = self
      .iter_blocks_i(inode_number)
      .zip(0..)
      .find_map(|(block_number, slot_index)| if block_number == NO_ADDRESS {
        Some(slot_index)
      } else {
        None
      }).ok_or_else(|| Errno::EIO(String::from("no more empty block slots in inode")))?;

    let free_slots_count = inode.direct_block_numbers.len() as AddressSize - (empty_slot + 1);

    // Guard for not enough empty slots in direct block number array
    // TODO: implement indirect blocks
    match free_slots_count {
      n if n < blocks_count => return Err(Errno::EIO(String::from("not enough empty block slots in inode"))),
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
      }).ok_or_else(|| Errno::EIO(String::from("no block slots used in inode - can't shrink")))?;

    let used_slots_count = inode.direct_block_numbers.len() as AddressSize - (first_used_slot + 1);

    // Guard for not enough used slots in direct block number array
    // TODO: implement indirect blocks
    match used_slots_count {
      n if blocks_count > n => return Err(Errno::EIO(String::from("not enough used slots in inode - can't shrink"))),
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

  fn iter_blocks_i(&self, inode_number: AddressSize) -> impl Iterator<Item = AddressSize> {
    let inode = self.read_inode(inode_number);

    inode.direct_block_numbers
      .into_iter()
  }

  // Errors:
  // ENOENT -> block_number does not exist
  fn write_block(&mut self, block: &Block, block_number: AddressSize) -> Result<(), Errno> {
    // Guard for block_number out of bounds
    if block_number > self.fs_info.blocks_count {
      return Err(Errno::ENOENT(String::from("write_block: block_number out of bounds")))
    }
    // Guard for block data being of invalid size
    if block.data.len() as AddressSize > self.fs_info.block_data_size {
      return Err(Errno::ENOENT(
          format!(
            "write_block: block.data should be {}, was {}",
            self.fs_info.block_data_size,
            block.data.len()
          )
      ))
    }

    // Read bytes from file
    let mut block_bytes = Vec::new();
    block_bytes.write(&block.data).unwrap();

    // Get absolute address of block
    let address = self.fs_info.first_block_address + block_number * self.fs_info.block_size;

    // Seek to it and write bytes
    self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.borrow_mut().write_all(&block_bytes).unwrap();

    Ok(())
  }
  
  fn write_inode(&mut self, inode: &INode, inode_number: AddressSize) -> Result<(), Errno> {
    // Guard for inoe_number out of bounds
    if inode_number > self.fs_info.inodes_count {
      return Err(Errno::ENOENT(String::from("write_inode: inode_number out of bounds")))
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
    inode_bytes.write(&inode.btime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.direct_block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    inode_bytes.write(&inode.indirect_block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();

    // Get absolute address of inode
    let address = self.fs_info.first_inode_address + inode_number * self.fs_info.inode_size;

    // Seek to it and write bytes
    self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.borrow_mut().write_all(&inode_bytes).unwrap();

    Ok(())
  }

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
    self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(0)).unwrap();
    self.fs_info.realfile.borrow_mut().write_all(&superblock_bytes).unwrap();

    Ok(())
  }

  fn read_block(&self, block_number: AddressSize) -> Block {
    let mut block_bytes = vec![0u8; self.fs_info.block_size.try_into().unwrap()];

    // Get absolute address of block
    let address = self.fs_info.first_block_address + block_number * self.fs_info.block_size;

    // Seek to it and read bytes
    self.fs_info.realfile.borrow_mut().seek(
      SeekFrom::Start(address.try_into().unwrap()).try_into().unwrap()
    ).unwrap();
    self.fs_info.realfile.borrow_mut().read_exact(&mut block_bytes).unwrap();

    // Return bytes as is, as it is raw data of a file
    Block {
      data: block_bytes,
    }
  }

  fn read_inode(&self, inode_number: AddressSize) -> INode {
    use std::mem::size_of;

    let mut inode_bytes = vec![0u8; self.fs_info.inode_size.try_into().unwrap()];

    // Get absolute address of inode
    let address = self.fs_info.first_inode_address + inode_number * self.fs_info.inode_size;

    // Seek to it and read bytes
    self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.borrow_mut().read_exact(&mut inode_bytes).unwrap();

    // Then parse bytes, draining from vector mutably
    let mode = FileMode(u16::from_le_bytes(inode_bytes.drain(0..size_of::<u16>()).as_slice().try_into().unwrap())); 
    let links_count = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); 
    let uid = Id::from_le_bytes(inode_bytes.drain(0..size_of::<Id>()).as_slice().try_into().unwrap()); 
    let gid = Id::from_le_bytes(inode_bytes.drain(0..size_of::<Id>()).as_slice().try_into().unwrap());
    let file_size = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let atime = UnixtimeSize::from_le_bytes(inode_bytes.drain(0..size_of::<UnixtimeSize>()).as_slice().try_into().unwrap());
    let mtime = UnixtimeSize::from_le_bytes(inode_bytes.drain(0..size_of::<UnixtimeSize>()).as_slice().try_into().unwrap());
    let ctime = UnixtimeSize::from_le_bytes(inode_bytes.drain(0..size_of::<UnixtimeSize>()).as_slice().try_into().unwrap());
    let btime = UnixtimeSize::from_le_bytes(inode_bytes.drain(0..size_of::<UnixtimeSize>()).as_slice().try_into().unwrap());
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
      btime,
      direct_block_numbers: direct_block_numbers.try_into().unwrap(),
      indirect_block_numbers: indirect_block_numbers.try_into().unwrap(),
      number: inode_number
    }
  }

  fn read_superblock(device_realpath: &str) -> Superblock {
    use std::mem::size_of;

    let mut superblock_bytes = vec![0u8; Superblock::size().try_into().unwrap()];

    let realfile = RefCell::new(
      std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(device_realpath)
        .unwrap()
    );

    realfile.borrow_mut().seek(SeekFrom::Start(0)).unwrap();
    realfile.borrow_mut().read_exact(&mut superblock_bytes).unwrap();

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

    // pub inode_number: AddressSize,
    // pub rec_len: u16,
    // pub name_len: u8,
    // pub name: String,

    let drain_one_entry = |data: &mut Vec<u8>| -> Result<DirectoryEntry, Errno> {
      let address_size = size_of::<AddressSize>();

      let inode_number = AddressSize::from_le_bytes(data.drain(0..address_size as usize).as_slice().try_into().or(Err(Errno::EILSEQ(String::from("can't parse inode_number"))))?);
      let rec_len = u16::from_le_bytes(data.drain(0..size_of::<u16>()).as_slice().try_into().or(Err(Errno::EILSEQ(String::from("can't parse rec_len"))))?);
      let name_len = u8::from_le_bytes(data.drain(0..size_of::<u8>()).as_slice().try_into().or(Err(Errno::EILSEQ(String::from("can't parse name_len"))))?);
      let name = String::from_utf8(data.drain(0..name_len as usize).collect()).or(Err(Errno::EILSEQ(String::from("can't parse name"))))?;

      // NOTICE: May be an off by 1 error here 
      if inode_number >= fs_info.inodes_count - 1 {
        return Err(Errno::EILSEQ(String::from("parse_directory: drain_one_entry: inode_number out of bounds")));
      } else if (rec_len as usize) < (address_size + size_of::<u16>() + size_of::<u8>() + size_of::<u8>()) {
        return Err(Errno::EILSEQ(String::from("parse_directory: drain_one_entry: rec_len is smaller than minimal")));
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
        .or(Err(Errno::EILSEQ(String::from("can't parse entries_count from dir"))))?
      );


    let mut entries = BTreeMap::new();

    for entry_index in 0..entries_count {
      match drain_one_entry(&mut data) {
        Ok(entry) => { 
          entries.insert(entry.name.to_owned(), entry); 
        },
        Err(errno) => {
          eprintln!("info: parse_directory: got to the end of directory: entry index: {entry_index} errno: {:?}", errno);
          break;
        },
      }
    }

    Ok(Directory::from(entries))
  }

  fn generate_fbl(&self) -> Vec<AddressSize> {
    let fbl_size_in_slots = 
      (self.fs_info.block_size / self.fs_info.address_size) * self.fs_info.blocks_needed_for_fbl;

    // Zip stub iterator with number of elements equal to
    // amount of slots in `fbl` sector
    // with actual free block numbers tailed with NO_ADDRESS
    (0..fbl_size_in_slots)
      .zip((0..self.fs_info.first_fbl_block_number).chain(std::iter::repeat(NO_ADDRESS)))
      .map(|(_, block_address)| block_address)
      .collect()
  }

  fn write_fbl(&mut self) {
    let fbl = self.generate_fbl();

    // let fbl_bytes: Vec<u8> = fbl.iter().flat_map(|x| x.to_le_bytes()).collect();

    // let address = self.fs_info.first_block_address + self.fs_info.first_fbl_block_number * self.fs_info.block_size;

    // self.fs_info.realfile.borrow_mut().seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    // self.fs_info.realfile.borrow_mut().write_all(&fbl_bytes).unwrap();

    // Write free blocks list to last N blocks
    // [ sb ... i1..iN ... b1[b1..bX ... fbl1..fblN]bN ]
    // Something like that ^
    // use itertools::Itertools;
    fbl
      .into_iter()
      .flat_map(AddressSize::to_le_bytes)
      .chunks(self.fs_info.block_size as usize)
      .into_iter()
      .zip(self.fs_info.first_fbl_block_number..self.fs_info.blocks_count)
      .for_each(|(block_bytes, block_number)| {
        let block = Block {
          data: block_bytes.collect(),
        };
        self.write_block(&block, block_number).unwrap();
      });
  }

  fn write_links_count_i(&mut self, inode_number: AddressSize, links_count: u32)
    -> Result<INode, Errno>
  {
    let mut inode = self.read_inode(inode_number);
    inode.links_count = links_count;
    self.write_inode(&inode, inode_number)?;

    Ok(inode)
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
        atime: unixtime(),
        mtime: unixtime(),
        ctime: unixtime(),
        btime: unixtime(),
        direct_block_numbers: [i % 5; 12],
        indirect_block_numbers: [i % 6; 3],
        number: i,
      });

      vec
    });

    // Write inodes to disk
    inodes
      .iter()
      .zip(inode_indices.clone())
      .for_each(|(inode, inode_number)| {
        e5fs.write_inode(inode, inode_number).unwrap();
      });

    // Read inodes from disk and assert equality
    inodes
      .iter()
      .zip(inode_indices.clone())
      .for_each(|(inode, _inode_number)| {
        let inode_from_file = e5fs.read_inode(inode.number);

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

        assert_eq!(*block, block_from_file, "block {block_number} should be correctly read");
      });
  }

  #[test]
  fn write_fbl_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    e5fs.write_fbl();

    let fbl = e5fs.generate_fbl();

    let fbl_from_file: Vec<AddressSize> = (e5fs.fs_info.first_fbl_block_number..e5fs.fs_info.blocks_count)
      .flat_map(|fbl_block_number| { 
        E5FSFilesystem::parse_block_numbers_from_block(
          &e5fs.read_block(fbl_block_number)
        ) 
      })
      .collect();

    assert_eq!(fbl, fbl_from_file, "fbl from file should match expected");
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
  
  // Should crash: only 16 inode slots (no auto replenishment
  // from disk) as of the time of writing this comments
  #[test]
  #[should_panic]
  fn allocate_file_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let range = 0..100;

    // Create a number of files files
    let inodes: Vec<INode> = range.clone().map(|_| e5fs.allocate_file().unwrap().1).collect();

    for (inode, i) in inodes.iter().zip(1..) {
      assert_eq!(inode.number, i as AddressSize, "file {} inode should be of inode_number {}, was {}", i, i, inode.number);
    }

    // Read these inodes back and compare
    let inodes_read: Vec<INode> = range.clone().map(|n| e5fs.read_inode(n + 1)).collect();
    assert_eq!(inodes, inodes_read, "allocated and read inodes should be equal");
  }

  #[test]
  fn write_and_read_directory_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    // Create 2 files
    let (file1_inode_num, _) = e5fs.allocate_file().unwrap();
    e5fs.write_data_i("hello world1".as_bytes().to_owned(), file1_inode_num, false).unwrap();
    let (file2_inode_num, _) = e5fs.allocate_file().unwrap();
    e5fs.write_data_i("hello world2".as_bytes().to_owned(), file2_inode_num, false).unwrap();

    let root_inode_number = e5fs.fs_info.root_inode_number;
    let expected_dir = Directory {
      entries_count: 2,
      entries: BTreeMap::from_iter(IntoIterator::into_iter([
        (String::from("hello-world1.txt"), DirectoryEntry::new(file1_inode_num, "hello-world1.txt").unwrap()),
        (String::from("hello-world2.txt"), DirectoryEntry::new(file2_inode_num, "hello-world2.txt").unwrap()),
      ])),
    };
    e5fs.write_dir_i(&expected_dir, root_inode_number).unwrap();

    let dir_from_disk = e5fs.read_as_dir_i(root_inode_number).unwrap();

    assert_eq!(dir_from_disk, expected_dir);
  }

  #[test]
  fn find_flb_block_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    let block_number = e5fs.find_block_in_fbl(|block_number| block_number != NO_ADDRESS).unwrap();

    assert_eq!(1, block_number);
  }

  #[test]
  fn lookup_path_works_simple() {
    let tempfile = mktemp().to_owned();
    mkenxvd("10M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();
    let root_inode = e5fs.read_inode(0);
    let mnt_inode = e5fs.allocate_file().unwrap().1;
    let bin_inode = e5fs.allocate_file().unwrap().1;
    let home_inode = e5fs.allocate_file().unwrap().1;
    e5fs.write_mode_i(home_inode.number, home_inode.mode.with_file_type(FileModeType::Dir as u8)).unwrap();
    e5fs.write_mode_i(bin_inode.number, bin_inode.mode.with_file_type(FileModeType::Dir as u8)).unwrap();
    e5fs.write_mode_i(mnt_inode.number, mnt_inode.mode.with_file_type(FileModeType::Dir as u8)).unwrap();
    let mnt_inode = e5fs.read_inode(mnt_inode.number);
    let bin_inode = e5fs.read_inode(bin_inode.number);
    let home_inode = e5fs.read_inode(home_inode.number);
    let expected_root_directory = {
      let mut dir = Directory::new();
      dir.insert(root_inode.number, ".").unwrap();
      dir.insert(root_inode.number, "..").unwrap();
      dir.insert(mnt_inode.number, "mnt").unwrap();
      dir.insert(bin_inode.number, "bin").unwrap();
      dir.insert(home_inode.number, "home").unwrap();
      dir
    };
    e5fs.write_dir_i(&expected_root_directory, root_inode.number).unwrap();

    let read_root_directory = e5fs.read_as_dir_i(root_inode.number).unwrap();
    assert_eq!(expected_root_directory, read_root_directory, "root directory should contain all created files");

    let read_root_vinode = e5fs.lookup_path("/").unwrap();
    let read_mnt_vinode = e5fs.lookup_path("/mnt").unwrap();
    let read_bin_vinode = e5fs.lookup_path("/bin").unwrap();
    let read_home_vinode = e5fs.lookup_path("/home").unwrap();

    assert_eq!(read_root_vinode.number, root_inode.number, "read_mnt_vinode should be equal to mnt_vinode");
    assert_eq!(read_mnt_vinode.number, 1, "read_mnt_vinode should be equal to mnt_vinode");
    assert_eq!(read_bin_vinode.number, 2, "read_bin_vinode should be equal to bin_vinode");
    assert_eq!(read_home_vinode.number, 3, "read_home_vinode should be equal to home_vinode");
    
    let nrv_inode = {
      let nrv_inode = e5fs.allocate_file().unwrap().1;
      e5fs.write_mode_i(nrv_inode.number, nrv_inode.mode.with_file_type(FileModeType::Dir as u8)).unwrap();
      e5fs.read_inode(nrv_inode.number)
    };
    let bashrc_inode = e5fs.allocate_file().unwrap().1;

    let expected_home_directory = {
      let mut dir = Directory::new();
      dir.insert(home_inode.number, ".").unwrap();
      dir.insert(root_inode.number, "..").unwrap();
      dir.insert(nrv_inode.number, "nrv").unwrap();
      dir
    };
    e5fs.write_dir_i(&expected_home_directory, home_inode.number).unwrap();

    let read_home_directory = e5fs.read_as_dir_i(home_inode.number).unwrap();
    assert_eq!(expected_home_directory, read_home_directory, "home directory should contain all created files");

    let expected_nrv_directory = {
      let mut dir = Directory::new();
      dir.insert(nrv_inode.number, ".").unwrap();
      dir.insert(home_inode.number, "..").unwrap();
      dir.insert(bashrc_inode.number, ".bashrc").unwrap();
      dir
    };
    e5fs.write_dir_i(&expected_nrv_directory, nrv_inode.number).unwrap();

    let read_nrv_directory = e5fs.read_as_dir_i(nrv_inode.number).unwrap();
    assert_eq!(expected_nrv_directory, read_nrv_directory, "nrv directory should contain all created files");

    let first_fbl_block = E5FSFilesystem::parse_block_numbers_from_block(&e5fs.read_block(e5fs.fs_info.first_fbl_block_number));
    let read_nrv_vinode = e5fs.lookup_path("/home/nrv").unwrap();
    let read_bashrc_vinode = e5fs.lookup_path("/home/nrv/.bashrc").unwrap();

    assert_eq!(read_nrv_vinode.number, nrv_inode.number, "read_nrv_vinode should be equal to nrv_inode");
    assert_eq!(read_bashrc_vinode.number, bashrc_inode.number, "read_bashrc_vinode should be equal to bashrc_vinode");
  }


  #[test]
  #[ignore]
  fn lookup_path_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("10M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::mkfs(tempfile.as_str(), 0.05, 4096).unwrap();

    // Count of files at root level (1)
    let first_layer_size: AddressSize = 2;
    // Count of files per dir on level (2)
    let second_layer_size: AddressSize = 2;

    let root_vinode = e5fs.lookup_path("/").unwrap();
    assert_eq!(root_vinode.number, 0, "inode_number of root_inode should be 0");
    let root_inode = e5fs.read_inode(root_vinode.number);

    assert_eq!(root_vinode, root_inode.into(), "looked up and read vinode and directly read inode should be equal");

    // Create 10 files
    let first_layer_files = (0..first_layer_size as AddressSize).fold(Vec::new(), |mut acc, cur| {
      let (inode_num, inode) = e5fs.allocate_file().unwrap();
      e5fs.write_data_i(cur.to_string().as_bytes().to_owned(), inode_num, false).unwrap();
      acc.push((inode_num, inode));
      acc
    });

    // Create 100 files to write 10 to each 
    // of 10 previous files (directories)
    let second_layer_files = (0..(second_layer_size*first_layer_size) as AddressSize).fold(Vec::new(), |mut acc, cur| {
      let (inode_num, inode) = e5fs.allocate_file().unwrap();
      let filename = format!("{}.txt", cur.to_string());
      e5fs.write_data_i(filename.as_bytes().to_owned(), inode_num, false).unwrap();
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
          let name = index.to_string().as_str().to_owned();
          (name.to_owned(), DirectoryEntry::new(*inode_num, &name).unwrap())
        })
        .collect(), 
    };
    e5fs.write_dir_i(&root_dir, root_inode_number).unwrap();

    first_layer_files
      .iter()
      .zip(second_layer_files.chunks(second_layer_size as usize))
      .enumerate()
      .for_each(|(first_layer_file_index, ((first_layer_file_inode_number, _first_layer_file_inode), inner_files))| {
        let dir = Directory {
          entries_count: second_layer_size,
          entries: inner_files
            .iter()
            .enumerate()
            .map(|(index, (inode_num, _inode))| {
              let name = index.to_string();
              (name.to_owned(), DirectoryEntry::new(*inode_num, &name).unwrap())
            })
            .collect(), 
        };
        e5fs.write_dir_i(&dir, *first_layer_file_inode_number).unwrap();

        inner_files
          .iter()
          .enumerate()
          .for_each(|(inner_index, (inner_f_inode_num, _innder_f_inode))| {
            let file_contents = format!("{}-{}", first_layer_file_index, inner_index);
            e5fs.write_data_i(file_contents.as_bytes().to_owned(), *inner_f_inode_num, false).unwrap();
          });
      });

    let first_layer_files_from_disk = (0..first_layer_size).fold(Vec::new(), |mut files, cur| {
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
    let mut e5fs = E5FSFilesystem::from(tempfile.as_str()).unwrap();

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
    inode1.mode = inode1.mode.with_file_type(FileModeType::Dir as u8);
    e5fs.write_inode(&inode1, inode1.number).unwrap();

    let vinode2 = e5fs.create_file("/test12/test2").unwrap();

    assert_eq!(vinode2.number, 2);
  }
}

// vim:ts=2 sw=2
