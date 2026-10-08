//! Offline read-only validation of a public v1 snapshot before using the v2 adapter.
use brioche_course_contract::{PublicLesson, neutral::NeutralLesson};
use std::{collections::BTreeSet, io::Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err("Expected one public v1 lesson-array JSON file".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[0])?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Snapshot exceeds 16 MiB".into());
    }
    let lessons: Vec<PublicLesson> = serde_json::from_slice(&bytes)?;
    if lessons.is_empty() || lessons.len() > 500 {
        return Err("Expected 1..500 public lessons".into());
    }
    let mut identities = BTreeSet::new();
    let mut assets = 0;
    let mut cues = 0;
    for lesson in &lessons {
        if !identities.insert((&lesson.id, lesson.revision)) {
            return Err("Duplicate immutable lesson identity".into());
        }
        NeutralLesson::try_from(lesson)
            .map_err(|error| format!("{}@{}: {error}", lesson.id, lesson.revision))?;
        assets += lesson.audio.len();
        cues += lesson
            .audio_tracks
            .iter()
            .map(|track| track.cues.len())
            .sum::<usize>();
    }
    println!(
        "Adapted {} public lessons; preserved {assets} audio descriptors and {cues} measured cues. No persistence or publication performed.",
        lessons.len()
    );
    Ok(())
}
