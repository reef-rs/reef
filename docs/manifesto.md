# Why Reef Exists

> *"The 40% you deleted isn't functionality. It's glue between things that shouldn't have been separate in the first place."*

## The honest origin story

Reef wasn't designed in a planning meeting. It was extracted from a production codebase that I built to escape the modern Next.js / Vercel / Neon / Drizzle / tRPC / Tailwind / TanStack-everything stack.

Every layer of that stack felt like progress when I added it. After eighteen months of shipping on it, I realized something darker: **none of those layers were solving my problems. They were solving problems the previous layer introduced.**

## The chain of self-justification

- **Server components** exist because client-side React is slow.
- **Server actions** exist because you need a way to call the backend from server components.
- **Vercel** exists because Next.js needs a Node.js host.
- **Neon** exists because Vercel's serverless model can't hold a database connection.
- **Drizzle** exists because you need TypeScript types for your Postgres schema, separate from your other types.
- **TanStack Query** exists because fetching data in React without a cache is a footgun.
- **TanStack DB** exists because TanStack Query plus complex state needs a normalized cache.
- **Electric / Convex** exist because that cache is hard to keep in sync with the server.

Every layer solves a problem the previous layer created. Remove the *first* layer (Node.js runtime) and the entire chain collapses.

## What collapses when you remove Node

- You don't need server components, because **WASM is fast.** Dioxus rendering on the client is competitive with native UI.
- You don't need server actions, because **you call the API directly** with typed RPC.
- You don't need Vercel, because **there's no Node.js to host.** A statically-linked Rust binary runs anywhere.
- You don't need Neon, because **the database is embedded.** libSQL/Turso lives in-process.
- You don't need Drizzle, because **your types are shared at compile time** through a `protocol` crate.
- You don't need TanStack DB, because **you can run actual SQLite in the browser via WASM** and `use_resource` over typed RPC handles the reactive cache.
- You don't need Electric/Convex, because **the same binary that runs in the cloud also runs at the edge** — sync is just one Rust process talking to another over Tailscale.

## What's left after the collapse

- **Rust** — the runtime
- **Dioxus** — the UI library
- **libSQL/Turso** — the database
- **Tailscale/Headscale** — the transport / authz fabric
- **Tower-Sessions** — auth primitives
- **Maybe NixOS** — for reproducible edge builds

That's it. Six things instead of twenty. And critically, **the same binary serves all roles** — cloud horizontal scaling, edge devices, thick client, mobile. Different `--mode` flags, same artifact.

## What Reef actually is

Reef is **the opinionated extraction** of this stack. It's not a single library; it's a curated combination plus the glue code that's specific to making them work together (sync engine, ACL→handler integration, scaffolding).

In that sense, Reef is more like T3 than like Next.js. There's no `import { something } from 'reef'` — Reef is the *brand*, the *scaffolder*, and the *opinionated combination*. The libraries underneath are real Rust crates with real maintainers, doing what they do.

## Who Reef is for

- People who've felt the JavaScript-fatigue thing and want out.
- People building real-world apps with edge-device requirements (cameras, sensors, industrial, medical, automotive, POS, anything with hardware).
- People who want a thick-client option *and* a cloud option, without rewriting the app.
- People who care about deployment ergonomics (small binaries, no Docker required, fast cold starts).
- People with strong opinions about doing things the right way, who are willing to pay the upfront cost of learning Rust to get the long-term cost savings of not fighting their stack.

## Who Reef is not for

- People shipping a CRUD SaaS on a deadline. Use T3 or Convex. Reef will slow you down for the first week.
- People who don't have time to learn Rust.
- People for whom "everything is in the cloud" is a feature, not a constraint.
- People allergic to opinionated frameworks. Reef is *very* opinionated.

## Reefer Madness

Building this way feels like clarity. From the outside, it looks like obsession. Both are accurate.

We accept the diagnosis.
