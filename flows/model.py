"""Claude replies for flows whose sessions run side by side, each chosen by a phrase in its request."""
import claude


def reply(marker, tool=None, arguments=None, text="flow done"):
    """One `tool` call with `arguments`, or else `text`, for a request holding `marker`; a marker
    of None answers requests in turn."""
    if tool:
        return claude.reply((None, tool, arguments), match=marker)
    return claude.reply(text=text, match=marker)


def tool(marker, name, /, **arguments):
    """A call of Razor's own tool `name` with `arguments`, for a request holding `marker`."""
    return reply(marker, f"mcp__razor__{name}", arguments)


def agent(marker, action, **arguments):
    return tool(marker, "agent", action=action, **arguments)


def message(marker, kind, **arguments):
    return tool(marker, "message", type=kind, **arguments)
