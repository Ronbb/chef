"""Private, offline predictions from a reviewed speech export; never registration/publication."""
import argparse
from decimal import Decimal, ROUND_FLOOR, ROUND_CEILING
import hashlib
import importlib.metadata
import io
import json
import math
import os
import platform
from pathlib import Path
import re
import struct
import sys
import tarfile
import unicodedata
import wave

# Private model/output storage belongs to the invoking workspace, not a nested
# framework checkout. Explicit override supports callers outside that workspace.
ROOT = Path(os.environ.get("CHEF_WORKSPACE_ROOT", os.getcwd())).resolve()
MODEL = json.loads(Path(__file__).with_name("model.json").read_text(encoding="utf-8"))
RUNTIME = json.loads(Path(__file__).with_name("runtime.json").read_text(encoding="utf-8"))
ALIASES = json.loads(Path(__file__).with_name("transcript-aliases.json").read_text(encoding="utf-8"))
MAX_ARCHIVE = 128 * 1024 * 1024
MAX_MEDIA = 16 * 1024 * 1024
MAX_JSON = 4 * 1024 * 1024
COMPILER = "speech-plan-1/uax29-1.13.3"
NEUTRAL_COMPILER = "speech-plan-2/author-scalar-1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value, sort=False):
    return json.dumps(value, ensure_ascii=False, sort_keys=sort,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON member")
        result[key] = value
    return result


def parse_json(data):
    require(len(data) <= MAX_JSON, "JSON exceeds size limit")
    result = json.loads(data, object_pairs_hook=unique_object,
                        parse_constant=lambda _: require(False, "nonfinite JSON number"))
    stack = [(result, 0)]
    while stack:
        item, depth = stack.pop()
        require(depth <= 64, "JSON exceeds depth limit")
        if isinstance(item, dict):
            stack.extend((v, depth + 1) for v in item.values())
        elif isinstance(item, list):
            stack.extend((v, depth + 1) for v in item)
        elif isinstance(item, str):
            require(not any(0xD800 <= ord(c) <= 0xDFFF for c in item), "invalid Unicode scalar")
    return result


def integer(value, minimum=0):
    return type(value) is int and value >= minimum


def hex_id(value, length):
    return isinstance(value, str) and re.fullmatch(r"[a-f0-9]{%d}" % length, value) is not None


def ordered(value, names):
    require(isinstance(value, dict) and set(value) == set(names), "unknown or missing plan fields")
    return {name: value[name] for name in names}


def plan_hash(plan):
    """Reproduce compiler v1 Serde struct order, not JSONB's alphabetical root order."""
    value = ordered(plan, ["compilerVersion", "lessonId", "lessonRevision", "sourceHash",
                           "planHash", "targets", "requests", "totalRequestCharacters"])
    value["planHash"] = ""
    targets = []
    for target in plan["targets"]:
        t = ordered(target, ["pointer", "blockId", "entryId", "text", "voice", "emotion", "generationKey", "words"])
        t["voice"] = ordered(t["voice"], ["characterId", "characterRevision", "voiceRevision"])
        t["words"] = [ordered(w, ["segmentId", "text", "segmentStart", "segmentEnd", "entryStart", "entryEnd"])
                      for w in t["words"]]
        targets.append(t)
    value["targets"] = targets
    # requests and their nested serde_json::Value objects use sorted maps.
    value["requests"] = json.loads(json_bytes(value["requests"], sort=True))
    return digest(json_bytes(value))


def normalize_wave(raw):
    require(44 <= len(raw) <= MAX_MEDIA and raw[:4] == b"RIFF" and raw[8:12] == b"WAVE", "invalid WAV")
    number = lambda offset: struct.unpack_from("<I", raw, offset)[0]
    streamed = number(4) == 2147483583
    require(streamed or number(4) == len(raw) - 8, "invalid RIFF length")
    output = bytearray(raw)
    offset, has_format, has_data = 12, False, False
    while offset < len(raw):
        require(len(raw) - offset >= 8, "truncated WAV chunk")
        size, start = number(offset + 4), offset + 8
        remaining = len(raw) - start
        if raw[offset:offset + 4] == b"fmt ":
            require(not has_format and size == 16 and remaining >= 16 and
                    raw[start:start + 4] == bytes([1, 0, 1, 0]) and
                    number(start + 4) == 24000 and number(start + 8) == 48000 and
                    raw[start + 12:start + 16] == bytes([2, 0, 16, 0]), "unsupported PCM format")
            has_format = True
        elif raw[offset:offset + 4] == b"data":
            require(not has_data and has_format and size > 0, "invalid PCM data")
            has_data = True
            if streamed and size > remaining and remaining > 0 and remaining % 2 == 0:
                struct.pack_into("<I", output, 4, len(raw) - 8)
                struct.pack_into("<I", output, offset + 4, remaining)
                return bytes(output)
        require(size <= remaining, "WAV chunk exceeds file")
        offset = start + size + size % 2
    require(not streamed and has_format and has_data and offset == len(raw), "invalid WAV layout")
    return bytes(output)


def pcm(data):
    require(normalize_wave(data) == data, "normalized WAV still requires repair")
    with wave.open(io.BytesIO(data), "rb") as w:
        require(w.getparams()[:3] == (1, 2, 24000) and w.getcomptype() == "NONE", "unsupported WAV")
        frames = w.getnframes()
        require(0 < frames <= 24000 * 180, "audio duration exceeds limit")
        payload = w.readframes(frames)
        require(len(payload) == frames * 2, "incomplete PCM samples")
        # Match the server's integer ceiling of actual decoded PCM duration.
        # A fractional final millisecond must not invalidate its fixed media receipt.
        return payload, (frames * 1000 + 23999) // 24000


def words_valid(text, words):
    require(isinstance(text, str) and 0 < len(text) <= 600 and isinstance(words, list) and 0 < len(words) <= 600, "invalid text/word list")
    previous = 0
    for word in words:
        require(set(word) == {"text", "start", "end"}, "unknown word fields")
        start, end = word["start"], word["end"]
        require(integer(start) and integer(end) and previous <= start < end <= len(text) and
                text[start:end] == word["text"] and not any(c.isspace() for c in word["text"]) and
                any(c.isalnum() for c in word["text"]), "invalid word range")
        require(not any(c.isalnum() or unicodedata.category(c).startswith("M") for c in text[previous:start]), "unmapped source letters")
        previous = end
    require(not any(c.isalnum() or unicodedata.category(c).startswith("M") for c in text[previous:]), "unmapped source letters")


def read_export(path, direct=False):
    with open(path, "rb") as file:
        raw = file.read(MAX_ARCHIVE + 1)
    require(len(raw) <= MAX_ARCHIVE, "archive exceeds size limit")
    members = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for entry in archive:
            require(entry.isreg() and not entry.pax_headers, "only plain regular members are accepted")
            require(entry.name == "manifest.json" or re.fullmatch(r"media/[a-f0-9]{64}\.wav", entry.name), "invalid archive member name")
            require(entry.name not in members, "duplicate archive member")
            require(0 < entry.size <= (MAX_JSON if entry.name == "manifest.json" else MAX_MEDIA), "member exceeds limit")
            require(len(members) < 2001, "too many archive members")
            members[entry.name] = archive.extractfile(entry).read(entry.size + 1)
            require(len(members[entry.name]) == entry.size, "incomplete archive member")
    require("manifest.json" in members, "missing manifest")
    manifest = parse_json(members["manifest.json"])
    fields = {"schemaVersion", "planId", "plan", "clips"}
    if direct:
        fields |= {"kind", "publicationPolicy", "humanListeningAsserted"}
        require(manifest.get("kind") == "brioche-speech-inputs" and
                manifest.get("publicationPolicy") == "owner-direct-publish" and
                manifest.get("humanListeningAsserted") is False, "unsupported direct input policy")
    require(set(manifest) == fields and manifest["schemaVersion"] == "1.0" and hex_id(manifest["planId"], 32), "unsupported manifest")
    plan = manifest["plan"]
    require(plan["compilerVersion"] in (COMPILER, NEUTRAL_COMPILER) and hex_id(plan["planHash"], 64) and
            hex_id(plan["sourceHash"], 64) and integer(plan["lessonRevision"], 1), "unsupported fixed plan")
    require(isinstance(plan["requests"], dict) and 0 < len(plan["requests"]) <= 1000 and
            isinstance(plan["targets"], list) and 0 < len(plan["targets"]) <= 1000, "invalid plan size")
    require(integer(plan["totalRequestCharacters"], 1), "invalid character budget")
    require(plan_hash(plan) == plan["planHash"], "fixed plan hash mismatch")
    clips = manifest["clips"]
    require(isinstance(clips, list) and len(clips) == len(plan["requests"]), "incomplete clip coverage")
    used, keys, ids = {"manifest.json"}, set(), set()
    for clip in clips:
        require(set(clip) == {"id", "generationKey", "result", "review", "words", "file", "providerFile"}, "unknown clip fields")
        key = clip["generationKey"]
        require(hex_id(key, 64) and key in plan["requests"] and key not in keys and
                hex_id(clip["id"], 32) and clip["id"] not in ids, "invalid/duplicate clip identity")
        keys.add(key); ids.add(clip["id"])
        request = plan["requests"][key]
        require(digest(json_bytes(request, sort=True)) == key, "generation key mismatch")
        text = request["parameters"]["input"]["text"]
        require(request["compilerVersion"] == plan["compilerVersion"] and request["parameters"]["model"] == "qwen-audio-3.1-tts-flash" and
                request["profile"]["locale"] in (("fr-FR",) if plan["compilerVersion"] == COMPILER else ("fr-FR", "yue-Hant-HK")) and
                request["parameters"]["input"]["voice"] == request["profile"]["voiceId"], "unsupported/mismatched speech request")
        if plan["compilerVersion"] == NEUTRAL_COMPILER:
            require(request.get("wordUnits") == clip["words"], "authored word units differ from fixed request")
        words_valid(text, clip["words"])
        review = clip["review"]
        if direct:
            require(review is None, "direct inputs must not assert a hearing record")
        else:
            require(isinstance(review, dict) and set(review) == {"actorId", "reason"} and integer(review["actorId"], 1) and
                    isinstance(review["reason"], str) and review["reason"].strip(), "missing source hearing record")
        result = clip["result"]
        for name, field in [("file", "sha256"), ("providerFile", "providerSha256")]:
            require(hex_id(result[field], 64) and clip[name] == f'media/{result[field]}.wav' and clip[name] in members, "missing/mismatched media")
            require(digest(members[clip[name]]) == result[field], "media hash mismatch")
            used.add(clip[name])
        audio = members[clip["file"]]
        require(normalize_wave(members[clip["providerFile"]]) == audio, "original/normalized pair mismatch")
        _, duration = pcm(audio)
        require(integer(result["byteLength"], 44) and integer(result["durationMs"], 1) and
                result["byteLength"] == len(audio) and result["durationMs"] == duration, "audio receipt mismatch")
    require(used == set(members), "unreferenced archive members")
    require({t["generationKey"] for t in plan["targets"]} == keys, "target coverage mismatch")
    require(sum(len(r["parameters"]["input"]["text"]) for r in plan["requests"].values()) ==
            plan["totalRequestCharacters"], "character budget mismatch")
    for target in plan["targets"]:
        request = plan["requests"][target["generationKey"]]
        require(target["text"] == request["parameters"]["input"]["text"] and target["voice"] == request["voice"], "target request mismatch")
        for word in target["words"]:
            require(integer(word["entryStart"]) and integer(word["entryEnd"]) and
                    0 <= word["entryStart"] < word["entryEnd"] <= len(target["text"]) and
                    target["text"][word["entryStart"]:word["entryEnd"]] == word["text"] and
                    integer(word["segmentStart"]) and integer(word["segmentEnd"], 1) and
                    word["segmentEnd"] - word["segmentStart"] == len(word["text"]), "invalid segment word range")
    return manifest, members, digest(raw)


def model_token(text):
    text = unicodedata.normalize("NFC", text).translate(str.maketrans({"’": "'", "ʼ": "'", "‘": "'"}))
    return "".join(c for c in text if c == "'" or unicodedata.category(c)[0] in "LN")


def transcript_aliases(words):
    # Only unambiguous, single-token cardinal spellings; no expansion or contraction.
    return [{"wordIndex": i, "sourceText": w["text"], "modelToken": ALIASES["cardinals"][w["text"]]}
            for i, w in enumerate(words) if w["text"] in ALIASES["cardinals"]]


def model_tokens(words, aliases=()):
    tokens = [model_token(w["text"]) for w in words]
    previous = -1
    for alias in aliases:
        require(set(alias) == {"wordIndex", "sourceText", "modelToken"}, "invalid transcript alias fields")
        i = alias["wordIndex"]
        require(type(i) is int and previous < i < len(words), "invalid transcript alias index")
        require(alias["sourceText"] == words[i]["text"] and
                ALIASES["cardinals"].get(alias["sourceText"]) == alias["modelToken"], "invalid cardinal alias")
        tokens[i] = alias["modelToken"]
        previous = i
    return tokens


def predictions(words, raw, duration, aliases=()):
    tokens = model_tokens(words, aliases)
    issues, output = [], []
    if len(raw) != len(words):
        return [], ["wordCountMismatch"]
    previous = 0
    for word, token, item in zip(words, tokens, raw):
        if item["text"] != token:
            issues.append("wordTextMismatch")
            continue
        start, end = item["startSeconds"], item["endSeconds"]
        if not (type(start) in (int, float) and type(end) in (int, float) and math.isfinite(start) and math.isfinite(end)):
            issues.append("invalidTime"); continue
        # Decimal string conversion avoids introducing overlap at e.g. 0.56s
        # through binary-float rounding; it does not repair model predictions.
        start_ms = int((Decimal(str(start)) * 1000).to_integral_value(rounding=ROUND_FLOOR))
        end_ms = int((Decimal(str(end)) * 1000).to_integral_value(rounding=ROUND_CEILING))
        if not (previous <= start_ms < end_ms <= duration):
            issues.append("invalidTimeRange"); continue
        previous = end_ms
        output.append({**word, "startMs": start_ms, "endMs": end_ms})
    return ([] if issues else output), sorted(set(issues))


def target_timings(target, words):
    indexed = {(w["start"], w["end"], w["text"]): w for w in words}
    result = []
    for source in target["words"]:
        match = indexed.get((source["entryStart"], source["entryEnd"], source["text"]))
        if match is None:
            return [], ["segmentWordBoundaryMismatch"]
        result.append({**source, "startMs": match["startMs"], "endMs": match["endMs"]})
    return result, []


def verify_model(directory):
    directory = Path(directory)
    require(directory.name == MODEL["revision"], "model directory must name fixed revision")
    for name, expected in MODEL["files"].items():
        with open(directory / name, "rb") as file:
            actual = hashlib.file_digest(file, "sha256").hexdigest()
        require(actual == expected, "model snapshot hash mismatch")


def load_model(directory):
    verify_model(directory)
    # Offline flags also cover nested loaders; inference never retrieves remote files.
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["HF_HUB_DISABLE_IMPLICIT_TOKEN"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    versions = {name: platform.python_version() if name == "python" else importlib.metadata.version(name) for name in RUNTIME}
    require(versions == RUNTIME, "unsupported alignment runtime; use pinned Windows CPU environment")
    from native import NativeAligner
    return NativeAligner(directory), versions


def align_export(manifest, members, archive_hash, model, versions, direct=False, spoken_cardinals=False):
    results = []
    for index, clip in enumerate(manifest["clips"]):
        payload, duration = pcm(members[clip["file"]])
        aliases = transcript_aliases(clip["words"]) if spoken_cardinals else []
        tokens = model_tokens(clip["words"], aliases)
        require(all(tokens), "empty model token")
        if manifest["plan"]["compilerVersion"] == NEUTRAL_COMPILER:
            require(not spoken_cardinals, "French cardinal aliases are unavailable for native author units")
            request = manifest["plan"]["requests"][clip["generationKey"]]
            raw = model.predict(payload, tokens, locale=request["profile"]["locale"], authored_units=True)
        else:
            raw = model.predict(payload, tokens)
        words, issues = predictions(clip["words"], raw, duration, aliases)
        targets = []
        for target in manifest["plan"]["targets"]:
            if target["generationKey"] != clip["generationKey"]:
                continue
            mapped, errors = target_timings(target, words) if not issues else ([], [])
            targets.append({"pointer": target["pointer"], "blockId": target["blockId"],
                            "entryId": target["entryId"], "words": mapped, "issues": errors})
        results.append({"clipId": clip["id"], "generationKey": clip["generationKey"],
                        "sha256": clip["result"]["sha256"], "durationMs": duration,
                        "rawPredictions": raw, "words": words, "issues": issues, "targets": targets})
        if aliases:
            results[-1]["transcriptAliases"] = aliases
        print(f"Aligned clip {index + 1}/{len(manifest['clips'])}", file=sys.stderr, flush=True)
    return {"schemaVersion": "1.0", "kind": "brioche-automatic-alignment-predictions" if direct else "brioche-alignment-predictions", "planId": manifest["planId"],
            "planHash": manifest["plan"]["planHash"], "sourceArchiveSha256": archive_hash,
            "engine": {**MODEL, "versions": versions, "device": "cpu", "dtype": "float32", "attention": "eager",
                       "transcript": ALIASES["policy"] if spoken_cardinals else "NFC source word units, apostrophes normalized; original scalar ranges retained; raw timestamp classes without interpolation"},
            "reviewRequired": not direct, "clips": results}


def write_private(path, value):
    path = Path(path).resolve()
    require(path.is_relative_to((ROOT / ".local/private").resolve()), "output must remain under .local/private")
    path.parent.mkdir(parents=True, exist_ok=True)
    require(path.parent.resolve().is_relative_to((ROOT / ".local/private").resolve()), "output parent escaped private directory")
    # Exclusive creation preserves existing reviewed artifacts and never overwrites files.
    with open(path, "x", encoding="utf-8") as file:
        os.chmod(path, 0o600)
        json.dump(value, file, ensure_ascii=False, allow_nan=False, indent=2)
        file.write("\n")
        file.flush(); os.fsync(file.fileno())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export", type=Path)
    parser.add_argument("--check", action="store_true", help="validate without loading model")
    parser.add_argument("--direct", action="store_true", help="use owner-authorized inputs without a human hearing declaration")
    parser.add_argument("--spoken-cardinals", action="store_true", help="explicit one-to-one French cardinal model tokens; retain raw predictions and source ranges")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--model", type=Path, default=ROOT / ".local/models/qwen3-forced-aligner-0.6b-hf" / MODEL["revision"])
    args = parser.parse_args()
    try:
        manifest, members, archive_hash = read_export(args.export, args.direct)
        if args.check:
            print(json.dumps({"valid": True, "requests": len(manifest["clips"]), "archiveSha256": archive_hash}))
            return 0
        require(args.output is not None and not args.output.exists(), "provide a new private output path")
        require(args.output.resolve().is_relative_to((ROOT / ".local/private").resolve()), "output must remain private")
        model, versions = load_model(args.model)
        report = align_export(manifest, members, archive_hash, model, versions, args.direct, args.spoken_cardinals)
        write_private(args.output, report)
        invalid = sum(bool(c["issues"] or any(t["issues"] for t in c["targets"])) for c in report["clips"])
        print(json.dumps({"clips": len(report["clips"]), "clipsWithIssues": invalid, "reviewRequired": not args.direct}))
        return 2 if invalid else 0
    except (ValueError, KeyError, TypeError, OSError, tarfile.TarError, wave.Error, RecursionError, OverflowError, RuntimeError, ImportError):
        print("Alignment failed validation; no registration or publication occurred.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
