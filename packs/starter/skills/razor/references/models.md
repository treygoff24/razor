# Models, providers and keys

Read `models.md`, `configuration.md` (Keys, Model routes and Providers) and `security.md`. `razor doctor` checks the default route and reports key presence without printing values. The person can use `razor setup` for an API-key provider or a sign-in, `razor login` for ChatGPT, and `razor keys` for optional web and voice keys; setup and keys need a terminal.

Provider keys, including model keys, can be stored as `VARIABLE = "value"` in `$RAZOR_HOME/keys.toml`, mode 0600. The server reads it at startup; an exported server environment variable wins. A provider's `key` setting names the variable, never the value. Use a secure handoff or have the person enter the secret; never print or put it in a message, tool call, settings file, or record. Restart is needed after changing keys; follow the restart guidance in troubleshooting.

A custom provider has a format, address and optional key variable in `[providers.<name>]`; its model is named `<provider>/<id>`. Set the model's context window when Razor does not know it. Check `models.md` for the formats, shipped providers, and examples, and run doctor to validate the actual route.

`provider_only` restricts Razor's own network activity to configured model providers; read `security.md` for the precise exceptions and restrictions. It also disables automatic release and skill downloads. A person can still explicitly run `razor update`. Do not disable the setting to make an unrelated feature work without the person's authorization.
