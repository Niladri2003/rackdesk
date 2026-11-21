//! Names of Linux users and groups, from `/etc/passwd` and `/etc/group`.
//!
//! The files are read when a name is needed. They are small, and this way a
//! new account shows without a restart. Accounts that come from a network
//! directory (LDAP and the like) are not in these files: they show as a
//! number.

/// One line of `/etc/passwd` or `/etc/group`: "name:x:id:...".
fn entries(text: &str) -> impl Iterator<Item = (&str, u32)> {
    text.lines().filter_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let id = fields.nth(1)?.parse().ok()?;
        (!name.is_empty() && !name.starts_with('#')).then_some((name, id))
    })
}

fn name_of(text: &str, id: u32) -> Option<String> {
    entries(text)
        .find(|(_, i)| *i == id)
        .map(|(name, _)| name.to_owned())
}

/// Accepts a name or a number, as `chown` does.
fn id_of(text: &str, name: &str) -> Option<u32> {
    name.parse()
        .ok()
        .or_else(|| entries(text).find(|(n, _)| *n == name).map(|(_, id)| id))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

pub fn user_name(uid: u32) -> Option<String> {
    name_of(&read("/etc/passwd"), uid)
}

pub fn group_name(gid: u32) -> Option<String> {
    name_of(&read("/etc/group"), gid)
}

pub fn user_id(name: &str) -> Option<u32> {
    id_of(&read("/etc/passwd"), name)
}

pub fn group_id(name: &str) -> Option<u32> {
    id_of(&read("/etc/group"), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n# a comment:x:9:9\nwww-data:x:33:33:www-data:/var/www:/usr/sbin/nologin\nbroken line\n:x:5:5\ndev:x:1000:1000::/home/dev:/bin/bash\n";

    #[test]
    fn names_and_ids_are_looked_up_both_ways() {
        assert_eq!(name_of(PASSWD, 33).as_deref(), Some("www-data"));
        assert_eq!(name_of(PASSWD, 9), None);
        assert_eq!(name_of(PASSWD, 5), None);
        assert_eq!(id_of(PASSWD, "dev"), Some(1000));
        assert_eq!(id_of(PASSWD, "1234"), Some(1234));
        assert_eq!(id_of(PASSWD, "nobody-here"), None);
        assert_eq!(id_of(PASSWD, ""), None);
    }
}
