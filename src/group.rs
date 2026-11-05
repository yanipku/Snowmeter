/*  Groups, as in Group=Clocks | weather: skins (and later meters and measures) can belong to
    several, ans the group bangs run on every member. Follows Rainmeter's Group class
    (Library/Group.cpp). */

use std::collections::HashSet;

#[derive(Default, Debug)]
pub struct Groups(HashSet<String>);

impl Groups {
    /*  Group=A|B|C: names are split at '|', trimed, and compared ignoring case.
        Empty names are skipped. */
    pub fn read_group(text: &str) -> Groups {
        Groups(text.split('|').map(str::trim).filter(|g| !g.is_empty()).map(str::to_uppercase).collect())
    }

    // More groups. Rainmeter joins the settings file's Group= and the skin's own this way
    pub fn add(&mut self, text: &str) {
        self.0.extend(Groups::read_group(text).0);
    }

    /*  Whether a bang's group argument matches: one name, names joined with & (in all of
        them) or with | (in any of them).mixing & and | matches nothing, as in Rainmeter. */
    pub fn matches(&self, query: &str) -> bool {
        let all = query.contains('&');
        if all && query.contains('|') {
            return false;
        }
        let sep = if all { '&' } else { '|' };
        let mut names = query.split(sep).map(|name| {
            let name = name.trim();
            !name.is_empty() && self.0.contains(&name.to_uppercase())
        });
        if all { names.all(|ok| ok) } else { names.any(|ok| ok) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_names() {
        let g = Groups::read_group("Clocks | weather ||");
        assert!(g.matches("clocks"));
        assert!(g.matches("WEATHER"));
        assert!(!g.matches("Other"));
        assert!(!g.matches(""));
    }

    #[test]
    fn any_and_all() {
        let g = Groups::read_group("A|B");
        assert!(g.matches("A|Z"));
        assert!(g.matches("A&B"));
        assert!(!g.matches("A&Z"));
        assert!(!g.matches("A&B|Z")); // & and | mixed: no match
    }
}