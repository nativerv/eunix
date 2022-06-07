use fancy_regex::Regex;
use itertools::Itertools;

use super::fs::Id;

#[derive(Debug)]
/// Serialized format: `name:password:uid:gid:comment:home:shell`
pub struct Passwd {
  pub name: String,
  pub password: String,
  pub uid: Id,
  pub gid: Id,
  pub comment: String,
  pub home: String,
  pub shell: String,
}

pub enum ParseError {
  BadLine,
  InvalidUid,
  InvalidGid,
  InvalidUserList,
}

impl Passwd {
  /// Parse `name:password:uid:gid:comment:home:shell`
  /// lines - invalid ones omitted
  pub fn parse_passwds(string: &str) -> Vec<Passwd> {
    string
      .lines()
      .flat_map(|line| {
        if !Regex::new("^.*:.*:.*:.*:.*:.*:.*$")
          .unwrap()
          .is_match(line)
          .unwrap() 
        {
          return Err(ParseError::BadLine);
        }

        let mut split = line.split(":");

        let name = split.next().unwrap_or("").to_owned();
        let password = split.next().unwrap_or("").to_owned();
        let uid = match split.next().map(str::parse::<Id>).ok_or(ParseError::InvalidUid)? {
          Ok(uid) => uid,
          Err(_) => {
            return Err(ParseError::InvalidUid);
          },
        };
        let gid = match split.next().map(str::parse::<Id>).ok_or(ParseError::InvalidGid)? {
          Ok(gid) => gid,
          Err(_) => {
            return Err(ParseError::InvalidGid);
          },
        };
        let comment = split.next().unwrap_or("").to_owned();
        let home = split.next().unwrap_or("").to_owned();
        let shell = split.next().unwrap_or("").to_owned();

        Ok(Passwd {
          name,
          password,
          uid,
          gid,
          comment,
          home,
          shell,
        })
      })
      .collect()
  }

  pub fn to_string(&self) -> String {
    let Passwd { name, password, uid, gid, comment, home, shell } = self;

    format!("{name}:{password}:{uid}:{gid}:{comment}:{home}:{shell}")
  }

  pub fn serialize_passwds(passwds: &[Passwd]) -> String {
    passwds
      .into_iter()
      .map(Self::to_string)
      .join("\n")
  }
}


#[derive(Debug)]
/// Serialized format: `name:gid:user1,user2,user3`
pub struct Group {
  pub name: String,
  pub gid: Id,
  pub user_list: Vec<String>,
}

impl Group {
  /// Parse `name:password:uid:gid:comment:home:shell`
  /// lines - invalid ones omitted
  pub fn parse_groups(string: &str) -> Vec<Group> {
    string
      .lines()
      .flat_map(|line| {
        if !Regex::new("^.*:.*:.*:.*$").unwrap().is_match(line).unwrap() {
          return Err(ParseError::BadLine);
        }

        let mut split = line.split(":");

        let name = split.next().unwrap_or("").to_owned();
        let gid = match split
            .next() 
            .map(str::parse::<Id>) 
            .ok_or(ParseError::InvalidUid)? 
        { 
            Ok(uid) => uid,
            Err(_) => {
              return Err(ParseError::InvalidGid);
            },
        };
        let user_list: Vec<String> = split
          .next()
          .unwrap_or("")
          .to_owned()
          .split(",")
          .map(ToOwned::to_owned)
          .collect();

        Ok(Group {
          name,
          gid,
          user_list,
        })
      })
      .collect()
  }

  pub fn to_string(&self) -> String {
    let Group { name, gid, user_list } = self;

    let user_list = user_list.join(",");

    format!("{name}:{gid}:{user_list}")
  }

  pub fn serialize_groups(groups: &[Group]) -> String {
    groups
      .into_iter()
      .map(Self::to_string)
      .join("\n")
  }
}

// vim:ts=2 sw=2
