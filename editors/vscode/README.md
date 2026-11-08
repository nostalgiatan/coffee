# Coffee Language (VS Code / Cursor)

TextMate grammar for Coffee source (`.cf`) and C declaration files (`.cfc`).

This is **not** CoffeeScript (`*.coffee`). Scope name is `source.coffee-lang` so the two do not clash.

`.cf` / `.cfc` may already be claimed by ColdFusion extensions. If highlighting looks wrong, disable that extension or in the status bar set the language mode to **Coffee**.

## Install (local, unpublished)

From this repo:

```bash
# VS Code
mkdir -p ~/.vscode/extensions
ln -sfn "$(pwd)/editors/vscode" ~/.vscode/extensions/coffee-lang

# Cursor
mkdir -p ~/.cursor/extensions
ln -sfn "$(pwd)/editors/vscode" ~/.cursor/extensions/coffee-lang
```

Reload the window (`Developer: Reload Window`). Open any `library/std/src/*.cf` file and you should see Coffee in the language picker.

To pack a VSIX later: `npx @vscode/vsce package` in this directory.
