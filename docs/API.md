# Hearth API keys

A Hearth API key lets one of your own apps talk to Claude through your Hearth server. It works like a normal API key, but the answers use your Claude plan's usage instead of API credits. Hearth keeps the request small (no tools, no files, no saved chat) and can pick the model and effort for you.

## 1. Make a key

Settings → **API keys** → New key (phone and desktop). Choose:

| Kind | Models | Effort | Good for |
| --- | --- | --- | --- |
| Low usage | haiku, sonnet | low, medium | chat bots, summaries, small tasks |
| High usage | sonnet, opus | high, xhigh, max | hard questions, code, long texts |
| General | every model | every level | your own tests |

You can change the list of **models** and **effort levels** of any kind yourself, and also set:
- **Memory**: *None* (every call is new), *One session* (every call continues the same Claude session, so it remembers the code and everything said before) or *Per name* (calls with the same `"session": "alice"` continue that session, other names start their own). With memory, only your newest message is sent after the first call. Add `"reset_session": true` to a call to start over.
- **Requests per day**: the key stops after that many (resets at midnight UTC).
- **Stop at session / weekly usage** (sliders): the key stops when this much of the 5-hour session or of the plan's week is used, so an app can never eat the last part of your limit.
- **Expires after**: a number of days. After that the key stops working.

The key (it starts with `hk_`) is shown **once**. Copy it then. Hearth only keeps a fingerprint, so a lost key cannot be shown again: make a new one. You can revoke or delete a key at any time.

Treat it like a password. Anyone with the key can use your plan's usage within its limits. Never put it in a web page or a phone app that other people install: keep it on your own server or in your own program.

## 2. Call it

The address is your Hearth server (for example `https://my-server.example.com`). The call is the same shape as Anthropic's Messages API.

```bash
curl https://my-server.example.com/v1/messages \
  -H "x-api-key: hk_YOUR_KEY" \
  -H "content-type: application/json" \
  -d '{ "model": "auto", "messages": [{ "role": "user", "content": "Explain DNS in two lines." }] }'
```

Answer:

```json
{ "id": "msg_hk_...", "type": "message", "role": "assistant", "model": "haiku",
  "content": [{ "type": "text", "text": "..." }], "stop_reason": "end_turn",
  "usage": { "input_tokens": 400, "output_tokens": 42 },
  "hearth": { "effort": "low", "picked_automatically": true } }
```

`Authorization: Bearer hk_...` works too.

### Python

```python
import requests
r = requests.post("https://my-server.example.com/v1/messages",
    headers={"x-api-key": "hk_YOUR_KEY"},
    json={"messages": [{"role": "user", "content": "Write a haiku about tea."}]})
print(r.json()["content"][0]["text"])
```

### JavaScript

```js
const r = await fetch("https://my-server.example.com/v1/messages", {
  method: "POST",
  headers: { "x-api-key": "hk_YOUR_KEY", "content-type": "application/json" },
  body: JSON.stringify({ messages: [{ role: "user", content: "Hello" }] }),
});
console.log((await r.json()).content[0].text);
```

### The Anthropic SDK

Point it at your server: `base_url` = your Hearth address, `api_key` = your `hk_` key. Use `client.messages.create(model="auto", max_tokens=1024, messages=[...])`. Streaming is not supported yet, so leave `stream` off.

## 3. Model and effort

- `"model": "auto"` (or leave it out): Hearth picks the lightest model and effort that fits the request, within what the key allows. Short and simple questions get the light ones, long or demanding ones (code, analysis, planning) get the best the key may use.
- `"model": "haiku" | "sonnet" | "opus"` (a full name like `claude-opus-4` also works): used when the key allows it. Otherwise you get a 403 that says which models the key may use.
- `"effort": "low" | "medium" | "high" | "xhigh" | "max" | "auto"`: same rules.
- `GET /v1/models` shows what the key may use, its daily limit, its weekly stop and when it expires.

## 4. What it can and cannot do

- Text in, text out. No images, no tools, no files. `system` (text) and a `messages` list of up to 200 items are supported. `max_tokens` is accepted but ignored.
- Claude has no tools and cannot touch your files. Without memory each request starts fresh and nothing is saved. With memory, the conversation is kept on your server in a session that belongs to that key, and only that key can continue it.
- At most 2 requests run at the same time on the server; others get 429 and can try again.
- A request that takes longer than 5 minutes is stopped (504).

## 5. Errors

| Code | Meaning |
| --- | --- |
| 400 | The request is wrong (the message says how). |
| 401 | Wrong, revoked or expired key. Many wrong keys in a row block your address for 15 minutes. |
| 403 | The model or effort is not allowed for this key. |
| 429 | The key's daily limit, session stop or weekly stop is reached, the plan's own limit is reached, or the server is busy. |
| 502 / 504 | Claude failed or took too long. |
