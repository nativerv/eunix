use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;

use crate::eunix::devfs::DeviceFilesystem;
use crate::eunix::fs::FilesystemType;
use crate::eunix::kernel::Times;
use crate::util::{self, unixtime};
use crate::{
  eunix::{
    e5fs::E5FSFilesystem,
    fs::{AddressSize, FileModeType, FileStat, Filesystem, VFS},
    kernel::{Args, Errno, Kernel},
  },
  machine::VirtualDeviceType,
};

pub const EXIT_ENOENT: AddressSize = 127;
pub const EXIT_SUCCESS: AddressSize = 0;
pub const EXIT_FAILURE: AddressSize = 1;

// FS reading stuff

pub fn ls(args: Args, kernel: &mut Kernel) -> AddressSize {
  if let Some(pathname) = args.get(1) {
    let _parent_dir = match VFS::parent_dir(pathname) {
        Ok(parent_dir) => parent_dir,
        Err(Errno::EINVAL(message)) => {
          println!("ls: invalid path: {message}");
          return 1;
        },
        Err(errno) => {
          println!("ls: invalid path: {errno:?}");
          return 1;
        },
    };

    let dir = match kernel.vfs.read_dir(&pathname) {
      Ok(dir) => dir,
      Err(Errno::ENOTDIR(_)) => {
        println!("ls: not a directory: {pathname}");
        return 1;
      },
      Err(errno) => {
        println!("ls: unexpected error: {errno:?}");
        return 1;
      }
    };

    for (child_name, _) in dir.entries {
      let child_pathname = format!("{pathname}/{child_name}");
      let vinode = kernel
        .vfs
        .lookup_path(&child_pathname)
        .expect(&format!("ls: we know that {child_pathname} exists"));

      // Print file type
      match vinode.mode.file_type().try_into().unwrap() {
        FileModeType::Dir => print!("d"),
        FileModeType::File => print!("-"),
        FileModeType::Sys => print!("s"),
        FileModeType::Block => print!("b"),
        FileModeType::Char => print!("c"),
      }

      // Print file permissions
      // User - read
      if util::get_bit_at(vinode.mode.user(), 2) {
        print!("r");
      } else {
        print!("-");
      }
      // User - write
      if util::get_bit_at(vinode.mode.user(), 1) {
        print!("w");
      } else {
        print!("-");
      }
      // User - execute
      if util::get_bit_at(vinode.mode.user(), 0) {
        print!("x");
      } else {
        print!("-");
      }
      // group - read
      if util::get_bit_at(vinode.mode.group(), 2) {
        print!("r");
      } else {
        print!("-");
      }
      // group - write
      if util::get_bit_at(vinode.mode.group(), 1) {
        print!("w");
      } else {
        print!("-");
      }
      // group - execute
      if util::get_bit_at(vinode.mode.group(), 0) {
        print!("x");
      } else {
        print!("-");
      }
      // others - read
      if util::get_bit_at(vinode.mode.others(), 2) {
        print!("r");
      } else {
        print!("-");
      }
      // others - write
      if util::get_bit_at(vinode.mode.others(), 1) {
        print!("w");
      } else {
        print!("-");
      }
      // others - execute
      if util::get_bit_at(vinode.mode.others(), 0) {
        print!("x");
      } else {
        print!("-");
      }

      print!("\t");

      // Links count
      print!("{}", vinode.links_count);

      print!("\t");

      // User and group owners
      print!("{} {}", vinode.uid, vinode.gid);

      print!("\t");

      print!("{}", vinode.file_size);

      print!("\t");

      // Date and time
      // Create a NaiveDateTime from the timestamp
      let naive = NaiveDateTime::from_timestamp(vinode.mtime as i64, 0);

      // Create a normal DateTime from the NaiveDateTime
      let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);

      // Format the datetime how you want
      let human_readable_date = datetime.format("%Y-%m-%d %H:%M:%S");
      print!("{}", human_readable_date);

      print!("\t");

      // Finally, file name, and newline for the next
      println!("{}", child_name);
    }
    0
  } else {
    1
  }
}

pub fn stat(args: Args, kernel: &mut Kernel) -> AddressSize {
  if let Some(pathname) = args.get(1) {
    println!("pathname_before_pass_to_bin_stat: {pathname}");
    let FileStat {
      mode,
      size,
      inode_number,
      links_count,
      uid,
      gid,
      block_size,
      atime,
      mtime,
      ctime,
      btime,
    } = match kernel.vfs.stat(&pathname) {
      Ok(stat) => stat,
      Err(errno) => {
        println!("stat: error: {errno:?}");
        return 1;
      }
    };
    let blocks_count = size.checked_div(block_size).unwrap_or(0);
    let file_type: FileModeType = mode.file_type().try_into().expect("should succeed");
    let file_mode_raw = mode.0;
    let atime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(atime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let mtime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(mtime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let ctime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(ctime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let btime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(btime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    println!("  File: {pathname}");
    println!("  Size: {size}\tBlocks: {blocks_count}\t{file_type}");
    println!("Device: <unknown>\tInode: {inode_number}\tLinks: {links_count}");
    println!("Access: {file_mode_raw:o}\tUid: {uid}\tGid: {gid}");
    println!("Access: {atime_human}");
    println!("Modify: {mtime_human}");
    println!("Change: {ctime_human}");
    println!(" Birth: {ctime_human}");
    0
  } else {
    1
  }
}

pub fn df(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn du(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn cat(args: Args, kernel: &mut Kernel) -> AddressSize {
  // #[derive(Debug, Parser)]
  // struct BinArgs {
  //   pathname: String,
  // }
  
  if args[1..].is_empty() {
    println!("cat: no files to concatenate");
    return 1;
  }

  let mut concatenated_bytes = Vec::new();

  for pathname in args[1..].to_vec() {
    // For every pathname check for errors and return or append bytes to result
    let mut bytes = match kernel.vfs.read_file(&pathname, AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::ENOENT(_)) => {
          println!("cat: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("cat: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(errno) => {
          println!("cat: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
    };
    concatenated_bytes.append(&mut bytes);
  }

  let utf8_string = match std::str::from_utf8(&concatenated_bytes) {
      Ok(utf8_string) => utf8_string,
      Err(utf8error) => {
        println!("cat: can't parse utf8: {utf8error}");
        return EXIT_FAILURE;
      },
  };

  println!("{utf8_string}");

  0
}

// FS writing stuff

pub fn mkfs_e5fs(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short, long, default_value_t = 4096)]
    block_data_size: AddressSize,

    #[clap(short, long, default_value_t = 0.1)]
    inode_table_percentage: f32,

    device_pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(parsed_args) => {
      let dev_pathname = parsed_args.device_pathname;
      let (mount_point, internal_pathname) = kernel.vfs.match_mount_point(&dev_pathname).unwrap();
      let mounted_fs = kernel.vfs.mount_points.get_mut(&mount_point).expect("VFS::lookup_path: we know that mount_point exist"); 

      let device_realpath = if mounted_fs.r#type == FilesystemType::devfs {
        match mounted_fs
          .driver
          .as_any()
          .downcast_ref::<DeviceFilesystem>()
          .expect("we know that mounted_fs.driver === instanceof DeviceFilesystem")
          .device_by_pathname(&internal_pathname) 
        {
            Ok(realpath) => realpath,
            Err(Errno::ENOENT(_)) => {
              println!("mkfs.e5fs: {dev_pathname}: No such file or directory");
              return EXIT_ENOENT;
            },
            Err(errno) => {
              println!("mkfs.e5fs: unexpected error: {errno:?}");
              return EXIT_FAILURE;
            },
        }
      } else {
        println!("mkfs.e5fs: {dev_pathname}: Not a device");
        return EXIT_FAILURE;
      };

      match E5FSFilesystem::mkfs(
        &device_realpath, 
        parsed_args.inode_table_percentage, 
        parsed_args.block_data_size
      ) {
        Ok(_) => EXIT_SUCCESS,
        Err(errno) => {
          println!("mkfs.e5fs: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    }
  }
}

pub fn mkdir(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      match kernel.vfs.create_dir(&pathname) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::ENOENT(_)) => {
          println!("mkdir: cannot create directory: '{pathname}': No such file or directory");
          EXIT_ENOENT
        },
        Err(errno) => {
          println!("mkdir: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn rmdir(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn touch(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => {
        match kernel.vfs.change_times(&pathname, Times {
          atime: unixtime(),
          mtime: vinode.mtime,
          ctime: unixtime(),
          btime: vinode.btime,
        }) {
          Ok(_) => EXIT_SUCCESS,
          Err(errno) => {
            println!("touch: unexpected error: {errno:?}");
            EXIT_FAILURE
          },
        }
        },
        Err(Errno::ENOENT(_)) => {
          match VFS
            ::parent_dir(&pathname)
            .and_then(|parent_pathname| kernel.vfs.lookup_path(&parent_pathname))
          {
            Ok(_) => {
              match kernel.vfs.create_file(&pathname) {
                Ok(_) => EXIT_SUCCESS,
                Err(errno) => {
                  println!("touch: unexpected error: {errno:?}");
                  EXIT_FAILURE
                },
              }
            },
            Err(Errno::ENOENT(_)) => {
              println!("touch: cannot touch '{pathname}': No such file or directory");
              EXIT_ENOENT
            },
            Err(errno) => {
              println!("touch: unexpected error: {errno:?}");
              EXIT_FAILURE
            },
          }
        },
        Err(errno) => {
          println!("touch: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn rm(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn mv(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn cp(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn chmod(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn chown(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

// System related stuff
pub fn uname(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn lsblk(args: Args, kernel: &mut Kernel) -> AddressSize {
  let device_table = kernel.devices();
  let mount_points = &kernel.vfs.mount_points;
  println!("{device_table:#?}");
  println!("mount_points: {mount_points:#?}");
  EXIT_SUCCESS
}

pub fn mount(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short = 't', long, default_value_t = FilesystemType::e5fs)]
    filesystem_type: FilesystemType,

    source: String,
    target: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mount: error: {message}");
      1
    }
    Ok(BinArgs {
      filesystem_type,
      source,
      target,
    }) => match kernel.mount(&source, &target, filesystem_type) {
      Ok(_) => 0,
      Err(Errno::EINVAL(message)) => {
        println!("mount: error: {message}");
        1
      }
      Err(_) => unreachable!(),
    },
  }
}

// User related stuff

pub fn id(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn whoami(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn su(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn useradd(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn usermod(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn userdel(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn groupmod(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

pub fn groupdel(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: error: {message}");
      1
    }
    Ok(BinArgs { pathname }) => 0,
  }
}

// vim:ts=2 sw=2