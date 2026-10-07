fn main() -> anyhow::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/generated");
    std::fs::create_dir_all(&root)?;
    std::fs::write(
        root.join("author-lesson.schema.json"),
        serde_json::to_string_pretty(&chef_engine::author_source::schema())?,
    )?;
    Ok(())
}
