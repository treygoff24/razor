# Models and sign-in

A Claude subscription or a ChatGPT subscription is enough to run Razor; no API key is needed. An API key for almost any provider works too, a company's AI gateway included ([Providers](#providers)). Razor reaches Claude through Anthropic's Agent SDK, GPT through OpenAI's Responses API, and every other model through an OpenAI-compatible chat endpoint; a provider says which of the three its models speak. Each session runs on one model at a time: switch with `/model` in the terminal app, start on one with `razor --model <name>`, or set the default with `model` in your settings. With no `model` set, new sessions start on `opus` at `high` effort.

The names in the table are model families. A family name follows that family's newest release as Razor's updates move it, so a session on `opus` uses a new Opus on its next turn after such an update; a full id with a version, such as `claude-opus-5-5`, stays on that release ([Family names](#family-names)).

| Family | Names | Sign-in | What it costs |
| --- | --- | --- | --- |
| Claude | `opus`, `sonnet`, `haiku`, `fable`, or a full `claude-…` id | `/login claude` or `razor login claude` (`razor login console` for a Console account) | Your Claude plan's usage limits; or API prices on an Anthropic Console account or key |
| GPT | `sol`, `terra`, `luna`, `astra`, or an exact GPT id | `razor login` | Your ChatGPT plan's usage limits |
| Gemini, DeepSeek, GLM | `gemini`, `deepseek`, `glm` | The provider's API key | The provider's API prices |
| Any provider's model | `<provider>/<id>`, such as `openai/gpt-6.1-sol` or `gateway/my-model` | The provider's API key, or none | Whatever that provider charges; nothing for a model on your own machine |

Effort, how much work the model puts into each step, is `low`, `medium`, `high`, `xhigh` or `max`, set with `/effort` or `--effort`. Some chat models stop below `max`; asking for more gives them their highest.

## Family names

Choose Fable, Opus, Sonnet, Haiku, Sol, Terra, Luna, Astra, Gemini, DeepSeek or GLM. The model
list offers these families first, followed by configured provider models.
Full ids with a version still work when typed and in settings, but the list does not offer them.
`/model Claude Fable`, `/model Fable` and `/model fable` all choose `fable`;
`GPT 6.1 Sol` and `sol` choose `sol`. Names ignore case, spaces and hyphens, vendor
words and version numbers. A lowercase full id with a version, such as
`gpt-6.1-sol` or `claude-fable-5-2`, stays on that release. Unknown names and
`<provider>/<id>` names pass through unchanged, leaving provider IDs intact.

A session keeps the family it was given, and its status shows the release that actually answered its last turn beside it when the provider reported one, after a restart too. Settings may name a model either way; a `[models]` row under a family name wins over one under the release the family stands for today, and the two rows are never combined.

## Claude

If Claude Code is already signed in on this machine, Razor is too: Razor runs Claude through the copy of Claude Code inside Anthropic's Agent SDK, and Claude Code keeps one sign-in for your user account, under `~/.claude` (in the login keychain on macOS). The `Claude:` line of `razor doctor` says whether it found one.

To sign in or replace an expired sign-in, type `/login` in the terminal app and choose Claude, or go straight there with `/login claude`. Open the link, then paste the code from Claude's page into the panel and press Enter if the browser does not complete it automatically. Escape cancels. From a shell, run `razor login claude`; usage counts against your Claude plan's limits.

Choose Anthropic Console in `/login` (or type `/login console`) to bill a Console account at API prices, or run `razor login console`. `razor setup` uses these same sign-ins and skips a working sign-in unless you ask for `--console`. On the phone and Mac app, an expired sign-in asks you to sign in again on the computer running that server.

There are three other ways in. Save `ANTHROPIC_API_KEY` in `keys.toml` ([Keys](#keys)) or export it where the server starts, and Claude sessions are billed at API prices. Name a model on the shipped `anthropic` provider, such as `anthropic/opus`, and that conversation uses the key whatever else is signed in ([Providers](#providers)); setup's API key choice for Anthropic does this. Or set `[claude] sign_in` to a command and its arguments, such as `sign_in = ["/home/you/bin/claude-token"]`, that prints a Claude sign-in token on its first line, and optionally an account name on its second, within 30 seconds. Razor runs it for each conversation it opens, with that conversation's session id in `RAZOR_SESSION`, so a script of yours can choose which of several Claude accounts each conversation uses. When Claude refuses the account because its organization turned off subscription access, that conversation's sign-in is dropped, and its next turn runs the command again; no restart is needed.

Claude sessions use Claude Code's own file tools, with Razor's shell in place of Claude Code's, beside Razor's own tools ([Concepts](concepts.md#tools)). Claude's own web search and fetch are always there, within your subscription; Razor's `search`, `fetch` and `answer` join them when a provider key is set.

## GPT

Sign in with your ChatGPT account:

```sh
razor login
```

It prints a link and a code: open `https://auth.openai.com/codex/device`, enter the code, and Razor saves the sign-in in `~/.razor/auth.json` and refreshes it by itself. Choose ChatGPT in `/login`, or type `/login chatgpt` in the terminal app, to do the same. This is the sign-in OpenAI's Codex uses, and usage counts against your ChatGPT plan; when the plan's limit is reached, the request fails at once rather than retrying.

Sol uses a 500,000-token window and hands over at 90% unless you set a window or threshold. A Sol setting of `window = 500000` or `threshold = 0.9` now matches the default and can be removed.

The names `sol`, `terra`, `luna` and `astra` track the latest models of those families. The family table in `src/profiles/models.rs` chooses each request's full ID; a session retains the family name, and its status shows the ID actually served. If you chose only ChatGPT in setup and your settings named no model, setup set `model = "sol"`.

On the sign-in, GPT runs in code mode by default, as Codex runs these models: it sees an `exec` tool that runs JavaScript against Razor's tools, so it can batch independent reads in one call, and a `wait` tool for a run that is still going. `[profiles.codex] code_mode = false` gives it plain function tools instead.

### With an OpenAI API key

To bill API use instead of a plan, name the model on the shipped `openai` provider: `openai/gpt-6.1-sol`, or any GPT id after `openai/`. It uses `OPENAI_API_KEY`, saved in `keys.toml` ([Keys](#keys)) or exported, and gets the same context window and speeds as the same id on the sign-in. Plain `sol` always means the ChatGPT sign-in. On a provider's Responses route GPT uses plain function tools, and turning code mode on there is refused.

## Keys

`~/.razor/keys.toml`, readable only by you, holds the API keys Razor uses, one line each, under the name of the variable a provider reads:

```toml
# ~/.razor/keys.toml
GATEWAY_API_KEY = "your-gateway-key"
OPENAI_API_KEY = "your-openai-key"
```

Setup's API key choice writes it for you. When the server starts it sets each variable its environment lacks from this file, so a key you export where the server starts wins, and it keeps every name the file lists out of agents' shells. Run `razor restart` after changing it. A key in this file needs no edit to the background service.

## Providers

A provider is where a group of models is reached: the API it speaks, its address, and the variable holding its key. Four ship with Razor:

| Provider | API | Address | Key |
| --- | --- | --- | --- |
| `anthropic` | Anthropic Messages | `https://api.anthropic.com` | `ANTHROPIC_API_KEY` |
| `openai` | OpenAI Responses | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| `google` | OpenAI-compatible chat | `https://generativelanguage.googleapis.com/v1beta/openai` | `GEMINI_API_KEY` |
| `runinfra` | OpenAI-compatible chat | `https://api.runinfra.ai/v1` | `RUNINFRA_API_KEY` |

The shipped chat models run on these: `gemini` (today `gemini-3.8-flash`) on `google`, `deepseek` (`deepseek-v4.1-flash-1017`) and `glm` (`glm-5.3-flash`) on `runinfra`. Any other provider is a `[providers.<name>]` table in your settings, and a session names its models `<provider>/<id>`, the id being what the provider calls the model:

```toml
# ~/.razor/config.toml
model = "gateway/my-model"

[providers.gateway]
format = "chat"
base_url = "https://gateway.example.com/v1"
key = "GATEWAY_API_KEY"
models = ["my-model", "my-other-model"]

[models."gateway/my-model"]
window = 200000
```

`format` is the API the provider speaks: `chat` (OpenAI-compatible chat, posting to `<base_url>/chat/completions`), `responses` (OpenAI's Responses API, `<base_url>/responses`) or `anthropic` (Anthropic's Messages API, which Razor runs through Claude Code with that conversation's address and key). Left out, it is `chat`. `key` names the variable holding the key, never the key itself; with none, Razor sends no key, as for a model on your own machine. `models` lists the ids `/model` offers; a session can name any other id too. A table with a shipped provider's name changes only the fields it sets, such as `[providers.google] base_url = "https://your-proxy.example.com/v1beta/openai"`.

Set the model's context `window` in tokens under `[models."<provider>/<id>"]`. Razor knows the window of the GPT ids and the shipped chat models, and Claude Code reports Claude's; of those it fills at most 500,000 tokens, so Claude's 1,000,000 is used whole only with `window = 1000000`. For any other model it does not guess, so without `window` it cannot tell when the context is filling and cannot rebuild in time, and `razor doctor` says so. A model on a chat provider gets plain file and shell tools (`read`, `write`, `edit` and `bash`, with `wait`, `send` and `kill` for running jobs) beside Razor's own; the shipped DeepSeek and GLM models get only the shell of those. Chat models have no web tools of their own. [Configuration](configuration.md#models) lists a model row's other fields.

## Checking what works

```sh
razor doctor
```

Its `Model:` line names your default model's route: Claude Code's sign-in or Razor's ChatGPT sign-in, or for a provider the provider, its address, the model id, where its key came from (the environment or `keys.toml`, never the key itself) and its context window. On a provider it then sends the model one short test request and says whether it was answered, or what failed (the key, the address, the API) and what to check. The last line is `Razor can run on <model>.` when it can. Doctor also says whether Claude's sign-in, Razor's ChatGPT sign-in and the web keys are present. It does not check whether a plan has usage left.
