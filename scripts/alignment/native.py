"""Transformers-native inference; keep raw classes, never interpolate timestamps."""
from decimal import Decimal


def raw_predictions(words, classes, quantum_ms):
    """Keep absent/extra timestamp slots visible to the prediction validator."""
    if quantum_ms != 80:
        raise ValueError("unexpected model timestamp quantum")
    result = []
    for index in range(max(len(words), (len(classes) + 1) // 2)):
        def seconds(offset):
            if offset >= len(classes):
                return "missing timestamp"
            value = classes[offset]
            if type(value) is not int or not 0 <= value < 5000:
                return "invalid timestamp class"
            return float(Decimal(value * quantum_ms) / 1000)
        result.append({"text": words[index] if index < len(words) else "unmatched timestamp",
                       "startSeconds": seconds(index * 2), "endSeconds": seconds(index * 2 + 1)})
    return result


class NativeAligner:
    def __init__(self, directory):
        import torch
        from transformers import AutoModelForTokenClassification, AutoProcessor
        torch.set_num_threads(4)
        self.torch = torch
        self.processor = AutoProcessor.from_pretrained(
            str(directory), local_files_only=True, trust_remote_code=False)
        self.model, info = AutoModelForTokenClassification.from_pretrained(
            str(directory), dtype=torch.float32, device_map="cpu", attn_implementation="eager",
            local_files_only=True, trust_remote_code=False, output_loading_info=True)
        if any(info.get(key) for key in ("missing_keys", "unexpected_keys", "mismatched_keys", "error_msgs")):
            raise ValueError("model weights do not match the fixed architecture")
        self.model.eval()
        if (self.processor.feature_extractor.sampling_rate != 16000 or
                self.processor.timestamp_segment_time != 80 or self.model.config.num_labels != 5000):
            raise ValueError("unexpected native aligner configuration")

    def predict(self, payload, tokens):
        import numpy as np
        import librosa
        # Export PCM is fixed 24 kHz; the official feature extractor requires 16 kHz.
        audio = np.frombuffer(payload, dtype="<i2").astype(np.float32) / 32768
        audio = librosa.resample(audio, orig_sr=24000, target_sr=16000, res_type="soxr_hq")
        inputs, word_lists = self.processor.prepare_forced_aligner_inputs(
            audio=[audio], transcript=" ".join(tokens), language="French",
            processor_kwargs={"return_tensors": "pt", "padding": True, "sampling_rate": 16000})
        inputs = inputs.to(self.model.device, self.model.dtype)
        with self.torch.inference_mode():
            output = self.model(**inputs)
        if len(word_lists) != 1 or output.logits.shape[0] != 1:
            raise ValueError("unexpected native prediction batch")
        classes = output.logits.argmax(dim=-1)[0][
            inputs["input_ids"][0] == self.model.config.timestamp_token_id].cpu().tolist()
        # decode_forced_alignment applies upstream _fix_timestamps. Deliberately keep
        # direct classifier values so overlap/zero/out-of-bounds predictions are reviewable.
        return raw_predictions(word_lists[0], classes, self.processor.timestamp_segment_time)
