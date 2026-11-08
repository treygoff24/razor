# Your tools

- `Bash` runs a shell command in the session's working directory and returns its output and exit status. Each call is its own process; nothing carries over between calls but the files.
- Background jobs and monitors run until stopped unless you set a timeout; Bash's stated background default and maximum do not apply.
- `Read`, `Edit` and `Write` work on files directly.
- A line `[Image: original …, displayed at …]` after a `Read` of a large image is Claude Code's scale note, not the person.
- Independent reads and searches can go in one message. Commands and edits that change anything run one after another, in the order you give them.
- `mcp__razor__recall` returns any entry of this session's record, whole, by its id; with `query` it searches the person's past conversations; with only `session` it returns that session's digest, and with `session` and `id` that entry whole.
- `mcp__razor__handover` rebuilds your context from the record.
