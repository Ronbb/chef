use crate::{AudioCue, Block, PublicLesson, Segment};
use std::collections::{BTreeMap, BTreeSet};

impl PublicLesson {
    pub(crate) fn validate_audio(&self) -> Result<(), String> {
        if self.audio.len() > 500 {
            return Err("/audio: too many recording assets".into());
        }
        if self.audio_tracks.len() > self.blocks.len() {
            return Err("/audioTracks: too many reading tracks".into());
        }
        let mut assets = BTreeMap::new();
        for (index, asset) in self.audio.iter().enumerate() {
            let extension = match asset.mime_type.as_str() {
                "audio/mpeg" => "mp3",
                "audio/wav" => "wav",
                _ => {
                    return Err(format!(
                        "/audio/{index}/mimeType: unsupported recording format"
                    ));
                }
            };
            if !crate::valid_content_id(&asset.asset_id) {
                return Err(format!("/audio/{index}/assetId: invalid recording ID"));
            }
            if !crate::valid_content_revision(asset.revision) {
                return Err(format!(
                    "/audio/{index}/revision: expected positive database revision"
                ));
            }
            if asset.duration_ms == 0 || asset.duration_ms > 1_800_000 {
                return Err(format!(
                    "/audio/{index}/durationMs: expected 1..1800000 milliseconds"
                ));
            }
            if asset.sha256.len() != 64
                || !asset
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(format!(
                    "/audio/{index}/sha256: expected lowercase SHA-256 hex"
                ));
            }
            if asset.url != format!("/api/audio/{}.{}", asset.sha256, extension) {
                return Err(format!(
                    "/audio/{index}/url: expected same-origin recording hash URL"
                ));
            }
            if asset.credit_zh.trim().is_empty() || asset.credit_zh.len() > 2000 {
                return Err(format!(
                    "/audio/{index}/creditZh: expected nonempty credit of at most 2000 bytes"
                ));
            }
            if assets.insert(&asset.asset_id, asset).is_some() {
                return Err(format!("/audio/{index}/assetId: duplicate recording ID"));
            }
        }
        let mut blocks = BTreeSet::new();
        let mut used_assets = BTreeSet::new();
        for (i, vocabulary) in self.knowledge.vocabulary.iter().enumerate() {
            if let Some(recording) = &vocabulary.recording {
                validate_knowledge(
                    recording,
                    &assets,
                    &format!("/knowledge/vocabulary/{i}/recording"),
                )?;
                used_assets.insert(&recording.asset.asset_id);
            }
        }
        for (gi, grammar) in self.knowledge.grammar.iter().enumerate() {
            for (ei, example) in grammar.examples.iter().enumerate() {
                if let Some(recording) = &example.recording {
                    validate_knowledge(
                        recording,
                        &assets,
                        &format!("/knowledge/grammar/{gi}/examples/{ei}/recording"),
                    )?;
                    used_assets.insert(&recording.asset.asset_id);
                }
            }
        }
        for (index, track) in self.audio_tracks.iter().enumerate() {
            let path = format!("/audioTracks/{index}");
            if !blocks.insert(&track.block_id) {
                return Err(format!("{path}/blockId: duplicate reading block track"));
            }
            if track.cues.is_empty() || track.cues.len() > 20_000 {
                return Err(format!("{path}/cues: expected 1..20000 intervals"));
            }
            let asset = assets
                .get(&track.asset_id)
                .ok_or_else(|| format!("{path}/assetId: unknown recording"))?;
            used_assets.insert(&track.asset_id);
            let entries: Vec<(&String, &Vec<Segment>)> = match self
                .blocks
                .iter()
                .find(|block| block.id() == track.block_id)
            {
                Some(Block::Dialogue { turns, .. }) => {
                    turns.iter().map(|e| (&e.id, &e.segments)).collect()
                }
                Some(Block::Article { paragraphs, .. }) => {
                    paragraphs.iter().map(|e| (&e.id, &e.segments)).collect()
                }
                _ => {
                    return Err(format!(
                        "{path}/blockId: recording requires a dialogue or article"
                    ));
                }
            };
            let mut targets = BTreeSet::new();
            let mut whole = BTreeMap::new();
            let mut segments = BTreeMap::new();
            for (ci, cue) in track.cues.iter().enumerate() {
                let cue_path = format!("{path}/cues/{ci}");
                if cue.start_ms >= cue.end_ms {
                    return Err(format!("{cue_path}/endMs: end must follow start"));
                }
                if cue.end_ms > asset.duration_ms {
                    return Err(format!(
                        "{cue_path}/endMs: interval outside recording duration"
                    ));
                }
                let (_, parts) = entries
                    .iter()
                    .find(|(id, _)| **id == cue.entry_id)
                    .ok_or_else(|| format!("{cue_path}/entryId: unknown entry"))?;
                let range = cue
                    .word_range
                    .as_ref()
                    .map(|range| (range.start, range.end));
                if !targets.insert((&cue.entry_id, &cue.segment_id, range)) {
                    return Err(format!("{cue_path}: duplicate audio target"));
                }
                match (&cue.segment_id, &cue.word_range) {
                    (None, None) => {
                        whole.insert(&cue.entry_id, (cue, ci));
                    }
                    (Some(segment_id), word) => {
                        let segment =
                            parts.iter().find(|s| &s.id == segment_id).ok_or_else(|| {
                                format!("{cue_path}/segmentId: segment does not belong to entry")
                            })?;
                        if let Some(word) = word {
                            let chars: Vec<_> = segment.text.chars().collect();
                            let (start, end) = (word.start as usize, word.end as usize);
                            if start >= end
                                || end > chars.len()
                                || chars[start..end].iter().any(|c| c.is_whitespace())
                                || !chars[start..end].iter().any(|c| c.is_alphanumeric())
                                || (start > 0 && chars[start - 1].is_alphanumeric())
                                || (end < chars.len() && chars[end].is_alphanumeric())
                            {
                                return Err(format!(
                                    "{cue_path}/wordRange: invalid Unicode scalar word boundaries"
                                ));
                            }
                        } else {
                            segments.insert((&cue.entry_id, segment_id), cue);
                        }
                    }
                    (None, Some(_)) => {
                        return Err(format!("{cue_path}/wordRange: a word requires a segment"));
                    }
                }
            }
            let mut previous_end = 0;
            for (id, _) in &entries {
                let (cue, ci) = whole
                    .get(id)
                    .ok_or_else(|| format!("{path}/cues: missing whole-entry interval for {id}"))?;
                if cue.start_ms < previous_end {
                    return Err(format!(
                        "{path}/cues/{ci}/startMs: whole-entry intervals overlap or disagree with reading order"
                    ));
                }
                previous_end = cue.end_ms;
            }
            for (ci, cue) in track.cues.iter().enumerate() {
                let cue_path = format!("{path}/cues/{ci}");
                let parent: &AudioCue = if cue.word_range.is_some() {
                    segments
                        .get(&(&cue.entry_id, cue.segment_id.as_ref().unwrap()))
                        .copied()
                        .ok_or_else(|| {
                            format!("{cue_path}/segmentId: word interval requires a parent segment interval")
                        })?
                } else {
                    whole.get(&cue.entry_id).unwrap().0
                };
                if cue.start_ms < parent.start_ms {
                    return Err(format!(
                        "{cue_path}/startMs: child interval starts before parent"
                    ));
                }
                if cue.end_ms > parent.end_ms {
                    return Err(format!(
                        "{cue_path}/endMs: child interval ends after parent"
                    ));
                }
            }
        }
        if used_assets.len() != assets.len() {
            let index = self
                .audio
                .iter()
                .position(|asset| !used_assets.contains(&asset.asset_id))
                .unwrap();
            return Err(format!(
                "/audio/{index}/assetId: recording has no reading or knowledge target"
            ));
        }
        Ok(())
    }
}

fn validate_knowledge(
    recording: &crate::KnowledgeRecording,
    assets: &BTreeMap<&String, &crate::AudioAsset>,
    path: &str,
) -> Result<(), String> {
    let registered = assets
        .get(&recording.asset.asset_id)
        .ok_or_else(|| format!("{path}/asset/assetId: unknown recording"))?;
    // Compare all public fields, including version, credit and same-origin hash URL.
    if serde_json::to_value(&recording.asset).unwrap() != serde_json::to_value(registered).unwrap()
    {
        return Err(format!(
            "{path}/asset: descriptor disagrees with the lesson recording registry"
        ));
    }
    if recording.start_ms >= recording.end_ms {
        return Err(format!("{path}/endMs: end must follow start"));
    }
    if recording.end_ms > registered.duration_ms {
        return Err(format!("{path}/endMs: interval outside recording duration"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioAsset, AudioTrack, AudioWordRange};
    fn fixture() -> PublicLesson {
        let mut lesson = crate::tests::fixture();
        let Block::Dialogue { id, turns, .. } = &mut lesson.blocks[1] else {
            panic!()
        };
        turns[0].segments[0].text = "🥐 Bonjour".into();
        let mut cues: Vec<_> = turns
            .iter()
            .enumerate()
            .map(|(i, e)| AudioCue {
                entry_id: e.id.clone(),
                segment_id: None,
                word_range: None,
                start_ms: i as u32 * 1000,
                end_ms: (i as u32 + 1) * 1000,
            })
            .collect();
        cues.push(AudioCue {
            entry_id: turns[0].id.clone(),
            segment_id: Some(turns[0].segments[0].id.clone()),
            word_range: None,
            start_ms: 100,
            end_ms: 600,
        });
        cues.push(AudioCue {
            entry_id: turns[0].id.clone(),
            segment_id: Some(turns[0].segments[0].id.clone()),
            word_range: Some(AudioWordRange { start: 2, end: 9 }),
            start_ms: 200,
            end_ms: 500,
        });
        lesson.audio_tracks = vec![AudioTrack {
            block_id: id.clone(),
            asset_id: "audio-bakery".into(),
            cues,
        }];
        let sha = "a".repeat(64);
        lesson.audio = vec![AudioAsset {
            asset_id: "audio-bakery".into(),
            revision: 1,
            sha256: sha.clone(),
            mime_type: "audio/mpeg".into(),
            duration_ms: 10_000,
            credit_zh: "Synthetic contract fixture; no recording file".into(),
            url: format!("/api/audio/{sha}.mp3"),
        }];
        lesson
    }
    #[test]
    fn validates_entry_segment_and_unicode_word_intervals() {
        fixture().validate().unwrap();
        let mut lesson = fixture();
        lesson.audio_tracks[0]
            .cues
            .last_mut()
            .unwrap()
            .word_range
            .as_mut()
            .unwrap()
            .end = 8;
        assert!(lesson.validate().unwrap_err().contains("wordRange"));
    }
    #[test]
    fn rejects_inconsistent_recordings_and_time_targets() {
        for case in 0..12 {
            let mut lesson = fixture();
            match case {
                0 => lesson.audio[0].duration_ms = 1,
                1 => lesson.audio[0].url = "https://other.test/recording.mp3".into(),
                2 => lesson.audio_tracks[0].asset_id = "missing".into(),
                3 => lesson.audio_tracks[0].cues[0].entry_id = "missing".into(),
                4 => {
                    lesson.audio_tracks[0].cues.remove(0);
                }
                5 => lesson.audio_tracks[0].cues[1].start_ms = 0,
                6 => {
                    let cue = lesson.audio_tracks[0].cues[0].clone();
                    lesson.audio_tracks[0].cues.push(cue);
                }
                7 => lesson.audio_tracks[0].cues.last_mut().unwrap().segment_id = None,
                8 => lesson.audio_tracks[0].cues.last_mut().unwrap().end_ms = 700,
                9 => {
                    let i = lesson.audio_tracks[0].cues.len() - 2;
                    lesson.audio_tracks[0].cues.remove(i);
                }
                10 => lesson.audio_tracks[0].block_id = lesson.blocks[0].id().into(),
                11 => lesson.audio_tracks.clear(),
                _ => unreachable!(),
            }
            assert!(lesson.validate().is_err(), "case {case}");
        }
    }
    #[test]
    fn reports_recording_fields_and_actual_unsorted_cue_positions() {
        use serde_json::json;
        let original = serde_json::to_value(fixture()).unwrap();
        for (pointer, value) in [
            ("/audio/0/assetId", json!("bad id")),
            ("/audio/0/revision", json!(0)),
            ("/audio/0/durationMs", json!(0)),
            ("/audio/0/sha256", json!("not-a-hash")),
            ("/audio/0/url", json!("https://other.test/file.mp3")),
            ("/audio/0/creditZh", json!(" ")),
            ("/audioTracks/0/cues/0/endMs", json!(0)),
            ("/audioTracks/0/cues/0/endMs", json!(10001)),
            ("/audioTracks/0/cues/1/startMs", json!(999)),
        ] {
            let mut source = original.clone();
            *source.pointer_mut(pointer).unwrap() = value;
            let lesson: PublicLesson = serde_json::from_value(source).unwrap();
            assert!(
                lesson
                    .validate()
                    .unwrap_err()
                    .starts_with(&format!("{pointer}:"))
            );
        }
        let mut lesson = fixture();
        // Cues need not be stored in reading order. The error must identify
        // the original item, not its position after ordering whole entries.
        lesson.audio_tracks[0].cues.swap(0, 1);
        lesson.audio_tracks[0].cues[0].start_ms = 999;
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with("/audioTracks/0/cues/0/startMs:")
        );

        let mut lesson = fixture();
        let word_index = lesson.audio_tracks[0].cues.len() - 1;
        lesson.audio_tracks[0].cues[word_index].end_ms = 700;
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with(&format!("/audioTracks/0/cues/{word_index}/endMs:"))
        );
        let mut lesson = fixture();
        lesson.audio_tracks[0].cues[word_index].start_ms = 50;
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with(&format!("/audioTracks/0/cues/{word_index}/startMs:"))
        );
        let mut lesson = fixture();
        lesson.audio_tracks[0].cues.remove(word_index - 1);
        assert!(lesson.validate().unwrap_err().starts_with(&format!(
            "/audioTracks/0/cues/{}/segmentId:",
            word_index - 1
        )));
        let mut lesson = fixture();
        lesson.audio_tracks.clear();
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with("/audio/0/assetId:")
        );
    }

    #[test]
    fn empty_audio_preserves_existing_public_document_shape() {
        let lesson = crate::tests::fixture();
        let value = serde_json::to_value(lesson).unwrap();
        assert!(value.get("audio").is_none());
        assert!(value.get("audioTracks").is_none());
        assert!(
            value["knowledge"]["vocabulary"][0]
                .get("recording")
                .is_none()
        );
    }

    #[test]
    fn knowledge_recordings_use_exact_registered_versions_and_bounded_intervals() {
        let mut lesson = fixture();
        let mut asset = lesson.audio[0].clone();
        asset.asset_id = "audio-knowledge".into();
        asset.sha256 = "b".repeat(64);
        asset.url = format!("/api/audio/{}.mp3", asset.sha256);
        asset.duration_ms = 1000;
        lesson.audio.push(asset.clone());
        let clip = crate::KnowledgeRecording {
            asset,
            start_ms: 50,
            end_ms: 900,
        };
        lesson.knowledge.vocabulary[0].recording = Some(clip.clone());
        lesson.knowledge.grammar[0].examples[0].recording = Some(clip.clone());
        lesson.validate().unwrap();
        let saved: crate::Vocabulary =
            serde_json::from_value(serde_json::to_value(&lesson.knowledge.vocabulary[0]).unwrap())
                .unwrap();
        assert_eq!(saved.recording.unwrap().asset.revision, clip.asset.revision);
        for altered in ["revision", "sha256", "url", "creditZh"] {
            let mut value = serde_json::to_value(&lesson).unwrap();
            value["knowledge"]["vocabulary"][0]["recording"]["asset"][altered] =
                if altered == "revision" {
                    serde_json::json!(2)
                } else {
                    serde_json::json!("different")
                };
            let bad: PublicLesson = serde_json::from_value(value).unwrap();
            assert!(
                bad.validate()
                    .unwrap_err()
                    .starts_with("/knowledge/vocabulary/0/recording/asset:")
            );
        }
        let mut bad = lesson.clone();
        bad.knowledge.vocabulary[0]
            .recording
            .as_mut()
            .unwrap()
            .end_ms = 1001;
        assert!(
            bad.validate()
                .unwrap_err()
                .starts_with("/knowledge/vocabulary/0/recording/endMs:")
        );
        let mut bad = lesson.clone();
        bad.knowledge.grammar[0].examples[0]
            .recording
            .as_mut()
            .unwrap()
            .start_ms = 900;
        assert!(
            bad.validate()
                .unwrap_err()
                .starts_with("/knowledge/grammar/0/examples/0/recording/endMs:")
        );
        let mut bad = lesson.clone();
        bad.audio.pop();
        assert!(
            bad.validate()
                .unwrap_err()
                .starts_with("/knowledge/vocabulary/0/recording/asset/assetId:")
        );
        lesson.knowledge.vocabulary[0].recording = None;
        lesson.knowledge.grammar[0].examples[0].recording = None;
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with("/audio/1/assetId:")
        );
    }
}
