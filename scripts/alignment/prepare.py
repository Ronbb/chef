"""Explicitly download the public fixed model; never read TTS keys or send audio."""
import os
from pathlib import Path

from align import MODEL, ROOT, verify_model


if __name__ == "__main__":
    os.environ["HF_HUB_DISABLE_IMPLICIT_TOKEN"] = "1"
    from huggingface_hub import snapshot_download
    target = ROOT / ".local/models/qwen3-forced-aligner-0.6b-hf" / MODEL["revision"]
    snapshot_download(MODEL["repository"], revision=MODEL["revision"], local_dir=str(target),
                      allow_patterns=list(MODEL["files"]), token=False, max_workers=2)
    verify_model(target)
    print("Fixed model snapshot verified. Inference can run offline.")
