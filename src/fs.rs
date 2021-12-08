use std::collections:BTreeMap;

pub type AddressSize = u64;
pub type FileMode = u16;
pub type FileMode = u16;
pub type FileDescriptor = AddressSize;

pub enum OpenMode {
  Read,
  Write,
  ReadWrite,
}

pub struct OpenFlags {
  mode: OpenMode,
  create: bool,
  append: bool,
}

pub struct VDirectoryEntry {
  num_inode: AddressSize,
  name: &str,
}

pub struct VINode {
  mode: u16,
  links_count: AddressSize,
  uid: u32,
  gid: u32,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
}

pub trait Filesystem {
  // Получить count байт из файловой
  // системы по указанному
  // pathname_from_fs_root,
  // либо ошибку если pathname_from_fs_root
  // не существует
  fn read_bytes(
    pathname_from_fs_root: &str,
    count: AddressSize
  ) -> Result<&[u8], Error>;

  fn read_dir(pathname: &str) -> &[VDirectoryEntry];

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(pathname: &str) -> VINode;
}

pub struct FileDescription {
  inode: VINode,
  flags: VINode,
}

pub struct VFS {
  mount_points: BTreeMap<&str, &dyn Filesystem>,
  open_files: BTreeMap<&str, FileDescription>,
}




// vim:ts=2 sw=2
