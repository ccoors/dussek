# itch.io

The releases go to itch.io as well as GitHub: the same five files, one channel each
(`packaging/itch.sh`; itch names the platform from the channel's name and serves a single file as it
is). `page.md` holds the page's fields and its description, `cover.png` its cover:

```sh
chromium --headless --window-size=630,500 --screenshot=packaging/itch/cover.png "file://$PWD/packaging/itch/cover.html"
```

## Setting it up, once

1. On itch.io, a new project with the fields of `page.md` (Draft at first), saved.
2. The current release pushed by hand, with your own butler (`butler login` once):

   ```sh
   v=0.3.1; mkdir -p dist/itch && gh release download "v$v" -D dist/itch
   packaging/itch.sh "$v" <user>/dussek dist/itch
   ```

   The channels appear on the page's Edit screen; give each its display name (`page.md`), then make
   the page Public.
3. For the next releases, the release workflow pushes them itself, once the repository has an API
   key (itch.io: Settings → API keys → Generate) and the target:

   ```sh
   gh secret set BUTLER_API_KEY            # pastes the key
   gh variable set ITCH_TARGET --body <user>/dussek
   ```

   Without either, its itch.io step says so and passes. It runs last: a failed push leaves the
   GitHub release and the website done; `packaging/itch.sh` by hand pushes them again.
