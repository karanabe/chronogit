//! Client-side allocation of compact parent lanes for the loaded commit page.

use crate::domain::{CommitSummary, ObjectId};

/// Builds a compact lane prefix for each commit using the parents already loaded for history.
pub(crate) fn graph_prefixes(commits: &[CommitSummary]) -> Vec<String> {
    let mut lanes: Vec<ObjectId> = Vec::new();
    let mut prefixes = Vec::with_capacity(commits.len());

    for commit in commits {
        let lane = lanes
            .iter()
            .position(|id| id == commit.id())
            .unwrap_or_else(|| {
                lanes.insert(0, commit.id().clone());
                0
            });
        let mut prefix = String::new();
        for index in 0..lanes.len() {
            prefix.push(if index == lane { '●' } else { '│' });
            prefix.push(' ');
        }
        if commit.parents().len() > 1 {
            prefix.push_str("─┬ ");
        }
        prefixes.push(prefix);

        let parents = commit.parents();
        if let Some(first_parent) = parents.first() {
            lanes[lane] = first_parent.clone();
        } else {
            lanes.remove(lane);
        }
        for (offset, parent) in parents.iter().skip(1).enumerate() {
            if !lanes.iter().any(|existing| existing == parent) {
                lanes.insert((lane + 1 + offset).min(lanes.len()), parent.clone());
            }
        }
        let mut index = 0;
        while index < lanes.len() {
            if lanes[..index].contains(&lanes[index]) {
                lanes.remove(index);
            } else {
                index += 1;
            }
        }
    }
    prefixes
}

#[cfg(test)]
mod tests {
    use super::graph_prefixes;
    use crate::domain::{CommitSummary, ObjectId};

    fn oid(character: char) -> ObjectId {
        ObjectId::parse(character.to_string().repeat(40)).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn allocates_and_rejoins_lanes_for_a_merge() {
        let commits = vec![
            CommitSummary::new(
                oid('a'),
                vec![oid('b'), oid('c')],
                String::new(),
                String::new(),
                "merge".to_owned(),
            ),
            CommitSummary::new(
                oid('c'),
                vec![oid('d')],
                String::new(),
                String::new(),
                "side".to_owned(),
            ),
            CommitSummary::new(
                oid('b'),
                vec![oid('d')],
                String::new(),
                String::new(),
                "main".to_owned(),
            ),
            CommitSummary::new(
                oid('d'),
                Vec::new(),
                String::new(),
                String::new(),
                "root".to_owned(),
            ),
        ];
        let prefixes = graph_prefixes(&commits);
        assert!(prefixes[0].contains("●"));
        assert!(prefixes[0].contains("┬"));
        assert!(prefixes[1].matches('│').count() >= 1);
        assert!(prefixes[2].contains("●"));
        assert!(prefixes[3].contains("●"));
    }
}
