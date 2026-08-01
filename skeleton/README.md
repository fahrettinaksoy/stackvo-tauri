# skeleton

Everything a StackVo workspace needs that is *not* the user's own code:
the service templates, the base compose file, and a `.env` with placeholders
where the credentials go.

It lives here because the app used to require a separate `stackvo` checkout to
read these from. The generator moved into this repository in Sprint 17; these
are the last inputs that had not. With them here, a workspace is a directory
the app can create rather than one the user has to clone.

**Placeholders only.** `.env.example` is committed, so nothing in it may be a
real credential — the contract calls this C-18. The copy this was taken from
carried a live Blackfire server token; it was replaced with an empty value
rather than moved, because a token in git history is a token that leaked.

Contents:

| path | what it is |
| --- | --- |
| `.env.example` | every setting the generator reads, with default values |
| `core/compose/base.yml` | Traefik and the shared network |
| `core/templates/services/*` | one compose fragment per catalog service |
| `core/templates/services/*/[a-z]*.tpl` | the config files rendered beside them |
