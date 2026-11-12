"""DESIGN.md's colours are the terminal's theme: the same names with the same colours, both ways."""
import re
from pathlib import Path

colours = Path("DESIGN.md").read_text().split("\ncolors:\n")[1].split("\ntypography:")[0]
design = dict(re.findall(r'^ +([\w-]+): "#([0-9a-f]{6})"$', colours, re.M))
table = Path("src/terminal/text.rs").read_text().split("theme! {\n")[1].split("\n}")[0]
theme = dict(re.findall(r'^ +[A-Z_]+ "([\w-]+)" 0x([0-9a-f]{6}),$', table, re.M))
assert (len(design), len(theme)) == (colours.count('"#'), table.count(" 0x")), "an entry this cannot read"
assert design == theme, set(design.items()) ^ set(theme.items())
