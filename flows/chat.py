"""Chat Completions replies, as the generic path's hosts stream them."""
import json

from standin import Reply

USAGE = {"prompt_tokens": 100, "completion_tokens": 10, "prompt_tokens_details": {"cached_tokens": 60}}


def reply(*deltas, finish="stop", usage=USAGE, done=True, cut=None, match=None):
    """A chunk per delta, then `finish` and `usage` unless None, then `[DONE]` if `done`. `cut`
    pauses the send inside its first occurrence, as a chunk boundary inside a character."""
    chunks = [{"choices": [{"index": 0, "delta": delta}]} for delta in deltas]
    chunks += [{"choices": [{"index": 0, "delta": {}, "finish_reason": finish}]}] * bool(finish)
    chunks += [{"choices": [], "usage": usage}] * bool(usage)
    body = "".join(f"data: {json.dumps(c, ensure_ascii=False)}\n\n" for c in chunks) + "data: [DONE]\n\n" * done
    body = body.encode()
    return Reply(body, cuts=(body.index(cut.encode()) + 1,) if cut else (), match=match)
