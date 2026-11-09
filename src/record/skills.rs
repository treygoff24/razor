//! Skill loads and restores share one ordered index; text stays in the record or on disk.

use super::{Error, Record, Summary, text};
use crate::{protocol::*, tools::Tools};

// Bound historical names while retaining enough space for every practical active context.
const SKILLS: usize = 256;
// Four tool outputs including wrappers and spill paths: three full-size skills fit.
const RESTORED_SKILLS: usize = 4 * SPILL;

impl Summary {
    pub(super) fn loaded_skill(&mut self, name: &str, entry: EntryId, at: Millis, how: SkillHow) {
        let loaded = SkillState {
            name: name.into(),
            entry,
            at,
            how,
            worn: None,
        };
        if let Some((first, skill)) = self.skills.iter_mut().find(|(_, s)| s.name == name) {
            *first = (*first).min(entry);
            if entry >= skill.entry {
                *skill = loaded;
            }
        } else {
            if self.skills.len() == SKILLS {
                let oldest = self
                    .skills
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (_, s))| (s.worn.is_none(), s.entry))
                    .map_or(0, |(i, _)| i);
                self.skills.remove(oldest);
            }
            self.skills.push((entry, loaded));
        }
        self.skills.sort_unstable_by_key(|(first, _)| *first);
    }

    /// The newest 32 loads in their first-load order, without copying the other states.
    pub(crate) fn shown_skills(&self) -> Vec<SkillState> {
        let mut newest: Vec<_> = self.skills.iter().collect();
        newest.sort_unstable_by_key(|(_, s)| std::cmp::Reverse(s.entry));
        newest.truncate(32);
        newest.sort_unstable_by_key(|(slot, _)| *slot);
        newest.into_iter().map(|(_, s)| s.clone()).collect()
    }
}

impl Record {
    pub(super) async fn restored_skills(
        &self,
        tools: &Tools,
        cut: EntryId,
        held: &[EntryId],
    ) -> Result<(String, Vec<RestoredSkill>), Error> {
        let at = EntryId(self.summary.last.0 + 1);
        let (mut restored, mut remaining) = (Vec::new(), RESTORED_SKILLS);
        let mut skills: Vec<_> = self
            .summary
            .skills
            .iter()
            .filter(|(_, s)| s.worn.is_none() && s.entry <= cut && !held.contains(&s.entry))
            .collect();
        skills.sort_unstable_by_key(|(_, s)| s.entry);
        for (first, skill) in skills.into_iter().rev() {
            let mut result = RestoredSkill {
                name: skill.name.clone(),
                how: skill.how,
                outcome: RestoreOutcome::OverCap,
            };
            let mut input = InputSkill {
                name: skill.name.clone(),
                text: String::new(),
            };
            let wrapper = text::skill(Some(&input), String::new()).chars().count();
            if remaining < wrapper {
                remaining = 0;
                restored.push((*first, (result, String::new())));
                continue;
            }
            let handed = match self.entry(skill.entry).await?.body {
                Body::Input(input) => input.skill,
                _ => None,
            };
            let (outcome, words) = match tools
                .restore_skill(at, &skill.name, handed.map(|s| s.text), remaining - wrapper)
                .await
            {
                Ok((outcome, output)) => {
                    input.text = output;
                    let words = matches!(outcome, RestoreOutcome::Restored { .. })
                        .then(|| text::skill(Some(&input), String::new()))
                        .unwrap_or_default();
                    (outcome, words)
                }
                Err(e) => (RestoreOutcome::Missing(e.to_string()), String::new()),
            };
            match outcome {
                RestoreOutcome::Restored { .. } => remaining -= words.chars().count(),
                RestoreOutcome::OverCap => remaining = 0,
                RestoreOutcome::Missing(_) => {}
            }
            result.outcome = outcome;
            restored.push((*first, (result, words)));
        }
        restored.sort_unstable_by_key(|(first, _)| *first);
        let restored: Vec<_> = restored.into_iter().map(|(_, result)| result).collect();
        Ok((
            text::restored_skills(&restored),
            restored.into_iter().map(|(s, _)| s).collect(),
        ))
    }
}
