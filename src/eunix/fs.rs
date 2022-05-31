use std::{collections::BTreeMap, any::Any, str::FromStr};
use core::fmt::{Debug, self};

use fancy_regex::Regex;
use itertools::Itertools;

use crate::util::{fixedpoint, unixtime};

use super::kernel::{Errno, UnixtimeSize};

pub type AddressSize = u32;
pub type Id = u16;

/// _По тонкому льду ((нет))_
///  
/// Use max address as indicator of no next block
/// Can be invalid in theory if we use exactly 2047 gigs of blocks,
/// after which the whole fs will not work anymore so who caresi guessb.
pub const NO_ADDRESS: AddressSize = AddressSize::MAX;
pub const EVERYTHING: AddressSize = AddressSize::MAX;
pub const NOBODY: Id = Id::MAX;

enum Devtype {
  File  = 0b000,
  Dir   = 0b001,
  Sys   = 0b010,
  Block = 0b011,
  Char  = 0b100,
}

//    free?
///   | unused
///   | |   filetype
///   | |   |   user
///   | |   |   |   group
///   | |   |   |   |   others
///   | |   |   |   |   |
///   f xxx ttt rwx rwx rwx
/// 0b0_000_000_110_000_000
/// Where:
/// filetype:
///   000 - file   100 - char
///   001 - dir    101 - unused
///   010 - sys    110 - unused
///   011 - block  111 - unused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMode(pub u16);
pub enum FileModeType {
  File = 0b000,
  Dir = 0b001,
  Sys = 0b010,
  Block = 0b011,
  Char = 0b100,
}

impl fmt::Display for FileModeType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      FileModeType::File => write!(f, "file"),
      FileModeType::Dir => write!(f, "directory"),
      FileModeType::Sys => write!(f, "system special"),
      FileModeType::Block => write!(f, "block device"),
      FileModeType::Char => write!(f, "character special"),
    }
  }
}

/// From raw u8's: 0b000 -> File etc
impl TryFrom<u8> for FileModeType {
  type Error = Errno;

  fn try_from(raw: u8) -> Result<Self, Self::Error> {
    match raw {
      x if x == FileModeType::File as u8 => Ok(FileModeType::File),
      x if x == FileModeType::Dir as u8 => Ok(FileModeType::Dir),
      x if x == FileModeType::Sys as u8 => Ok(FileModeType::Sys),
      x if x == FileModeType::Block as u8 => Ok(FileModeType::Block),
      x if x == FileModeType::Char as u8 => Ok(FileModeType::Char),
      _ => Err(Errno::EINVAL(format!("cannot convert raw file type to enum: this error should not occur, bruh"))),
    }
  }
}

/// Default is user read-write only.
impl Default for FileMode {
  fn default() -> Self {
      // NOTICE: 1 f bit may be wrong
      Self(0b1_000_000_110_000_000)
  }
}

impl FileMode {
  #[allow(dead_code)]
  pub fn new(raw: u16) -> Self {
    Self(raw)
  }

  #[allow(dead_code)]
  pub fn zero() -> Self {
    Self(0b0000000_000_000_000)
  }

  pub fn free(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[0..1], 2).expect(&format!("can't parse in free: {}", &current))
  }
  
  pub fn file_type(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[4..7], 2).expect(&format!("can't parse in type: {}", &current))
  }

  pub fn user(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[7..10], 2).expect(&format!("can't parse in user: {}", &current))
  }

  pub fn group(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[10..13], 2).expect(&format!("can't parse in group: {}", &current))
  }

  pub fn others(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);
    
    u8::from_str_radix(&current[13..16], 2).expect(&format!("can't parse in others: {}", &current))
  }

  pub fn with_free(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:01b}", mask);

    current.replace_range(0..1, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in free: {}", &current)))
  }
  
  pub fn with_file_type(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);

    current.replace_range(4..7, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in type: {}", &current)))
  }

  pub fn with_user(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);

    current.replace_range(7..10, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in user: {}", &current)))
  }

  pub fn with_group(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);

    current.replace_range(10..13, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in group: {}", &current)))
  }

  pub fn with_others(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);
    
    current.replace_range(13..16, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in others: {}", &current)))
  }

  pub fn get_raw(&self) -> u16 {
    self.0
  }

  /// gets the bit at position `n`. Bits are numbered from 0 (least significant) to 31 (most significant).
  fn get_bit_at(input: u32, n: u8) -> bool {
    if n < 32 {
      input & (1 << n) != 0
    } else {
      false
    }
  }
}

impl std::ops::Add for FileMode {
  type Output = Self;

  fn add(self, rhs: Self) -> Self::Output {
    Self(self.0 + rhs.0)
  }
}

pub type FileDescriptor = AddressSize;

/// struct stat {
///   dev_t     st_dev;         /* ID of device containing file */
///   ino_t     st_ino;         /* Inode number */
///   mode_t    st_mode;        /* File type and mode */
///   nlink_t   st_nlink;       /* Number of hard links */
///   uid_t     st_uid;         /* User ID of owner */
///   gid_t     st_gid;         /* Group ID of owner */
///   dev_t     st_rdev;        /* Device ID (if special file) */
///   off_t     st_size;        /* Total size, in bytes */
///   blksize_t st_blksize;     /* Block size for filesystem I/O */
///   blkcnt_t  st_blocks;      /* Number of 512B blocks allocated */
/// 
///   /* Since Linux 2.6, the kernel supports nanosecond
///      precision for the following timestamp fields.
///      For the details before Linux 2.6, see NOTES. */
/// 
///   struct timespec st_atim;  /* Time of last access */
///   struct timespec st_mtim;  /* Time of last modification */
///   struct timespec st_ctim;  /* Time of last status change */
/// 
///   #define st_atime st_atim.tv_sec      /* Backward compatibility */
///   #define st_mtime st_mtim.tv_sec
///   #define st_ctime st_ctim.tv_sec
/// };
#[derive(Debug)]
pub struct FileStat {
  pub mode: FileMode,
  pub size: AddressSize,
  pub inode_number: AddressSize,
  pub links_count: AddressSize,
  pub uid: u16,
  pub gid: u16,
  pub block_size: AddressSize,
  pub atime: UnixtimeSize,
  pub mtime: UnixtimeSize,
  pub ctime: UnixtimeSize,
  pub btime: UnixtimeSize,
}

#[derive(Debug, Clone, Copy)]
pub enum OpenMode {
  Read,
  Write,
  ReadWrite,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenFlags {
  pub mode: OpenMode,
  pub create: bool,
  pub append: bool,
}
impl OpenFlags {
  pub fn mode(&self) -> OpenMode {
    self.mode
  }
  pub fn create(&self) -> bool {
    self.create
  }
  pub fn append(&self) -> bool {
    self.append
  }

  pub fn new(mode: OpenMode, create: bool, append: bool) -> Self {
    Self {
      mode,
      create,
      append,
    }
  }
  pub fn with_mode(mut self, mode: OpenMode) -> Self {
    self.mode = mode;
    self
  }
  pub fn with_create(mut self, create: bool) -> Self {
    self.create = create;
    self
  }
  pub fn with_append(mut self, append: bool) -> Self {
    self.append = append;
    self
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct VDirectory {
  pub entries: BTreeMap<String, VDirectoryEntry>,
}
impl VDirectory {
  pub fn new() -> Self {
    Self {
      entries: BTreeMap::new(),
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct VDirectoryEntry {
  pub inode_number: AddressSize,
  pub name: String,
}
impl VDirectoryEntry {
  pub fn new(inode_number: AddressSize, name: &str) -> Self {
    Self {
      inode_number,
      name: name.to_owned(),
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct VINode {
  /// File mode
  pub mode: FileMode,
  /// Links count (how many times inode listed in directories)
  pub links_count: AddressSize,
  /// User owner
  pub uid: Id,
  /// Group owner
  pub gid: Id,
  /// File size (in bytes, i guess)
  pub file_size: AddressSize,
  /// Last access time
  pub atime: UnixtimeSize,
  /// Last contents change time
  pub mtime: UnixtimeSize,
  /// Last inode change time
  pub ctime: UnixtimeSize,
  /// Birth (creation) time (non-standard)
  pub btime: UnixtimeSize,
  /// Inode number 
  pub number: AddressSize,
}

impl VINode {
  fn new() -> Self {
    // NOTICE: this is unfinished artifact
    Self {
      mode: FileMode::zero()
        .with_free(0)
        .with_file_type(Devtype::Char as u8)
        .with_user(0b111)
        .with_group(0b000)
        .with_others(0b000),
      links_count: 1,
      uid: 0,
      gid: 0,
      file_size: 0,
      atime: unixtime(),
      mtime: unixtime(),
      ctime: unixtime(),
      btime: unixtime(),
      number: 0,
    }
  }
}

pub trait Filesystem {
  // Получить count байт из файловой
  // системы по указанному
  // pathname_from_fs_root,
  // либо ошибку если pathname_from_fs_root
  // не существует
  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno>;

  fn create_dir(&mut self, pathname: &str)
    -> Result<VINode, Errno>;

  fn read_file(&mut self, pathname: &str, count: AddressSize)
    -> Result<Vec<u8>, Errno>;

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<VINode, Errno>;

  fn read_dir(&self, pathname: &str)
    -> Result<VDirectory, Errno>;

  fn stat(&self, pathname: &str)
    -> Result<FileStat, Errno>;

  fn change_mode(&mut self, pathname: &str, mode: FileMode)
    -> Result<(), Errno>;

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str)
    -> Result<VINode, Errno>;

  fn name(&self) -> String;
  fn as_any(&mut self) -> &mut dyn Any;
}

impl Debug for dyn Filesystem {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
      write!(f, "Filesystem {{ {} }}", self.name())
  }
}

impl Filesystem for VFS {
  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get_mut(&mount_point).expect("VFS::create_file: we know that mount_point exist");  
    mounted_fs.driver.create_file(&internal_pathname)
  }

  fn create_dir(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get_mut(&mount_point).expect("VFS::create_dir: we know that mount_point exist");  
    mounted_fs.driver.create_dir(&internal_pathname)
  }

  fn read_file(&mut self, pathname: &str, _count: AddressSize)
    -> Result<Vec<u8>, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get_mut(&mount_point).expect("VFS::read_file: we know that mount_point exist");  
    mounted_fs.driver.read_file(&internal_pathname, EVERYTHING)
  }

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<VINode, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get_mut(&mount_point).expect("VFS::write_file: we know that mount_point exist");  
    mounted_fs.driver.write_file(&internal_pathname, data)
  }

  fn read_dir(&self, pathname: &str)
    -> Result<VDirectory, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get(&mount_point).expect("VFS::read_dir: we know that mount_point exist");  

    // Guard for Not a directory
    match mounted_fs.driver.stat(&internal_pathname)? {
      stat if stat.mode.file_type() != FileModeType::Dir as u8 
        => return Err(Errno::ENOTDIR(String::from("read_dir: not a directory"))),
      _ => (),
    }

    mounted_fs.driver.read_dir(&internal_pathname)
  }

  fn stat(&self, pathname: &str)
    -> Result<FileStat, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get(&mount_point).expect("VFS::stat: we know that mount_point exist");  
    mounted_fs.driver.stat(&internal_pathname)
  }

  fn change_mode(&mut self, pathname: &str, mode: FileMode)
    -> Result<(), Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get_mut(&mount_point).expect("VFS::change_mode: we know that mount_point exist");  
    mounted_fs.driver.change_mode(&internal_pathname, mode)
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчит на маунт-поинты и вызывает lookup_path("/internal/path") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str)
    -> Result<VINode, Errno> {
    let (mount_point, internal_pathname) = self.match_mount_point(pathname)?;
    let mounted_fs = self.mount_points.get(&mount_point).expect("VFS::lookup_path: we know that mount_point exist");  
    mounted_fs.driver.lookup_path(&internal_pathname)
  }

  fn name(&self) -> String {
    String::from("vfs")
  }

fn as_any(&mut self) -> &mut dyn Any {
    self
  }
}

#[derive(Debug, Clone)]
pub struct FileDescription {
  pub vinode: VINode,
  pub flags: OpenFlags,
  pub pathname: Option<String>,
}
impl FileDescription {
  // pub fn new() {
  //   pub vinode: VINode,
  //   pub flags: OpenFlags,
  //   pub pathname: String,
  //   FileDescription {
  //     vinode:
  //   }
  // }
}

#[derive(Debug)]
pub struct VFS {
  pub mount_points: BTreeMap<String, MountedFilesystem>,
  pub open_files: BTreeMap<String, FileDescription>,
}

#[derive(Debug)]
pub struct MountedFilesystem {
  pub r#type: FilesystemType,
  pub driver: Box<dyn Filesystem>
}

impl MountedFilesystem {
  pub fn driver_as() {
  }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FilesystemType {
  devfs,
  binfs,
  // procfs(ProcessFilesystem),
  // sysfs(SysFilesystem),
  e5fs,
  // tmpfs(MemFilesystem),
}

impl FromStr for FilesystemType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "devfs" => Ok(FilesystemType::devfs),
      "binfs" => Ok(FilesystemType::binfs),
      // "procfs" => Ok(FilesystemType::procfs),
      // "sysfs" => Ok(FilesystemType::sysfs),
      "e5fs" => Ok(FilesystemType::e5fs),
      // "tmpfs" => Ok(FilesystemType::tmpfs),
      _ => Err(format!("<unknown_fs>")),
    }
  }
}

impl fmt::Display for FilesystemType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      FilesystemType::devfs => write!(f, "devfs"),
      FilesystemType::binfs => write!(f, "binfs"),
      // FilesystemType::procfs => write!(f, "procfs"),
      // FilesystemType::sysfs => write!(f, "sysfs"),
      FilesystemType::e5fs => write!(f, "e5fs"),
      // FilesystemType::tmpfs => write!(f, "tmpfs"),
    }
  }
}

impl VFS {
  pub fn parent_dir(pathname: &str) -> Result<String, Errno> {
    let (everything_else, final_component) = VFS::split_path(pathname)?;
    Ok(format!("/{}", everything_else.join("/")))
  }
  pub fn add_open_file(&mut self, pathname: &str, file_description: &FileDescription) -> Result<(), Errno> {
    self.open_files.insert(pathname.to_owned(), file_description.clone());

    Ok(())
  }

  pub fn remove_open_file(&mut self, pathname: &str, file_description: &FileDescription) -> Result<(), Errno> {
    self.open_files.remove(pathname).ok_or_else(|| Errno::ENOENT(String::from("vfs: no such file was open"))).map(|_| ())
  }

  /// Returns: `(mount_point_pathname, internal_pathname)`
  pub fn match_mount_point(&self, pathname: &str)
    -> Result<(String, String), Errno> 
  {
    let (mount_point, _mounted_fs) = self.mount_points
      .iter()
      .sorted_by(|(key1, _), (key2, _)| key1.len().cmp(&key2.len()))
      .rev()
      .map(|(key, value)| (key.clone(), value))
      .find(|(mount_point, _mounted_fs)| {
        let re = Regex::new(&format!("^{}", mount_point)).unwrap();
        re.is_match(pathname).expect("fix yo regex nerd (is_match)")
      })
      .ok_or_else(|| Errno::ENOENT(String::from("VFS::lookup_path: no such file or directory")))?;

    let regex = Regex::new(&format!("^{}", mount_point))
      .expect("VFS::match_mount_point: regex can't be invalid because of regex::escape");
    let mut internal_pathname = regex.replace_all(pathname, "").to_string();

    // Wut?
    // if internal_pathname == "" {
    //   internal_pathname = String::from(".");
    // }

    // Add leading slash - required by (my) standart
    let internal_pathname = format!("/{}", internal_pathname);


    Ok((mount_point.to_owned(), internal_pathname))
  }

  /// "/"            -> `([], "/")`
  /// "/foo"         -> `([], "foo")`
  /// "/foo/bar"     -> `(["foo"], "bar")`
  /// "/foo/bar/baz" -> `(["foo", "bar"], "baz")`
  pub fn split_path(pathname: &str) -> Result<(Vec<String>, String), Errno> {
    // Guard for empty `pathname`
    match &pathname {
      pathname if pathname.chars().count() == 0 => { 
        return Err(Errno::EINVAL(String::from("zero-length path")))
      },
      pathname if pathname
        .chars()
        .nth(0)
        .unwrap() != '/' => return Err(Errno::EINVAL(String::from("path must start with '/'"))),
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

    let everything_else: Vec<String> = pathname
      .split('/')
      .take(pathname.split('/').count() - 1)
      .map(|piece| piece.to_owned())
      .collect();
    let final_component: String = pathname
      .split('/')
      .last()
      .expect("fs::split_path: we know that there is element").to_owned();

    match pathname.split('/').count() {
      // E.g. with '/test1' we have vec!["", "test1"]
      1 => Ok((Vec::new(), final_component)),
      _ => Ok((everything_else, final_component)),
    }
  }
}


#[cfg(test)]
mod tests {
  use crate::{util::{mkenxvd, mktemp}, eunix::{e5fs::E5FSFilesystem, devfs::DeviceFilesystem}};

use super::*;

  #[test]
  fn file_mode_works() {
    let expected: u16 = 0b1_000_011_101_110_001;

    let filemode = FileMode::zero()
      .with_free(0b1)
      .with_file_type(0b011)
      .with_user(0b101)
      .with_group(0b110)
      .with_others(0b001);

    assert_eq!(filemode.get_raw(), expected);
  }

}

#[cfg(test)]
mod vfs_split_path_tests {
  use super::*;

  #[test]
  fn split_path_root() {
    assert_eq!(VFS::split_path("/").unwrap(), ((Vec::new(), String::from("/"))));
  }
  #[test]
  fn split_path_only_slashes() {
    assert_eq!(VFS::split_path("//////").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(VFS::split_path("/////").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(VFS::split_path("////").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(VFS::split_path("///").unwrap(), ((Vec::new(), String::from("/"))));
    assert_eq!(VFS::split_path("//").unwrap(), ((Vec::new(), String::from("/"))));
  }
  #[test]
  fn split_path_valid_1() {
    assert_eq!(VFS::split_path("/test1").unwrap(), ((Vec::new(), String::from("test1"))));
  }
  #[test]
  fn split_path_valid_2() {
    assert_eq!(VFS::split_path("/test1/test2").unwrap(), ((vec![String::from("test1")], String::from("test2"))));
  }
  #[test]
  fn split_path_valid_3() {
    assert_eq!(VFS::split_path("/test1/test2/test3").unwrap(), ((vec![String::from("test1"), String::from("test2")], String::from("test3"))));
  }
  #[test]
  fn split_path_valid_multiple_slashes() {
    assert_eq!(VFS::split_path("//test1//test2///test3////").unwrap(), ((vec![String::from("test1"), String::from("test2")], String::from("test3"))));
  }
  #[test]
  fn split_path_valid_onechar_1() {
    assert_eq!(VFS::split_path("/a").unwrap(), ((Vec::new(), String::from("a"))));
  }
  #[test]
  fn split_path_valid_onechar_2() {
    assert_eq!(VFS::split_path("/a/b").unwrap(), ((vec![String::from("a")], String::from("b"))));
  }
  #[test]
  fn split_path_valid_onechar_3() {
    assert_eq!(VFS::split_path("/a/b/c").unwrap(), ((vec![String::from("a"), String::from("b")], String::from("c"))));
  }
  #[test]
  fn split_path_zero_length() {
    match VFS::split_path("") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL(String::from("zero-length path"))),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_1() {
    match VFS::split_path("test1") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL(String::from("path must start with '/'"))),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_1_trailing() {
    match VFS::split_path("test1/") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL(String::from("path must start with '/'"))),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_2() {
    match VFS::split_path("test1/test2") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL(String::from("path must start with '/'"))),
      _ => unreachable!(),
    };
  }
  #[test]
  fn split_path_invalid_3() {
    match VFS::split_path("test1/test2/test3") {
      Err(errno) => assert_eq!(errno, Errno::EINVAL(String::from("path must start with '/'"))),
      _ => unreachable!(),
    };
  }
}
// vim:ts=2 sw=2
