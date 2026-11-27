# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

> Not a website: a GNOME desktop integration. The UI is drawn by GNOME Shell (St/Clutter, styled with Shell CSS) and GTK4/libadwaita. "web" is the closest value the schema allows; the design language to follow is GNOME's own (Shell theme + Adwaita HIG).

## Stack

- **Daemon:** Rust (`copilotd`, tokio + zbus) on the session bus as `io.github.GnomeCopilot`.
- **Inference:** Cactus engine `libcactus.so`, loaded at runtime with `dlopen`. Pinned and SHA-256 verified, downloaded from Hugging Face on first use.
- **Shell UI:** GNOME Shell extension (GJS, ESM, GNOME 45+). It draws the edge notch, the tray icon and the floating popover.
- **Preferences:** Rust GTK4 + libadwaita app (`copilot-settings`).
- Decided with the user: extension + Rust daemon architecture, FFI to libcactus, and a per-risk confirmation policy.

## Users

GNOME desktop users who want to control their computer by typing (and later by speaking) a short request. Their job is to get a system task done ("abre o Firefox", "ativa o modo escuro", "configurações de Wi-Fi") without hunting through menus, and without sending anything to the cloud. The primary user speaks Brazilian Portuguese. English requests must work too.

## Product Purpose

A local, private AI assistant that lives at the edge of the GNOME desktop. The user summons it from a movable side notch (like Samsung's Edge Panel) or from a tray icon. It opens as a floating panel that closes when the user clicks outside. They type a request, and the assistant carries out the system action. Success means the request is understood and done in under a second, the panel feels like part of GNOME, and nothing leaves the machine.

## Positioning

Fully on-device. The models are Cactus Compute's tiny models (Needle ≈35 MB for actions; later Whistle for speech and Mirage for vision). They run in a sub-2 MB engine with about 100 MB of RAM and need no GPU or cloud account. The app ships with **no model**: the user picks one and downloads it from inside the tool.

## Operating Context

- It sits beside the user's work all day, so it must stay out of the way until summoned and get out of the way afterwards.
- It follows GNOME light and dark styles, accent color, and the reduced-motion setting.
- Actions run on the user's live session: launching apps, Settings pages, volume, dark style, night light, Do Not Disturb, screenshots, closing apps, lock, and power.

## Capabilities and Constraints

- **v1:** the notch and/or tray trigger (configurable), a floating popover, a preferences app, model download/removal/activation, and system actions via Needle 3.
- **Confirmation policy:** safe actions run immediately. Sensitive ones (closing apps, lock, power, commands) show a confirmation card. Low-confidence guesses appear as "Você quis dizer…?" suggestions. Shell commands are off by default and always require confirmation.
- **Grounding:** deterministic rules fix arguments the request states explicitly and lift confidence for trigger-routed calls. Eval harness: `crates/copilotd/src/agent.rs` (`eval`).
- **Later:** Whistle (voice requests, dictation into any app) and Mirage (see the screen with permission, click and type).
- Wayland: only the Shell can pin UI to a screen edge or close it on an outside click, which is why the UI lives in an extension.

## Brand Commitments

- It must look and behave like part of GNOME. The binding reference is GNOME's Quick Settings menu: rounded floating panel, pill toggles, Shell typography and colors, smooth short animations.
- UI language: Brazilian Portuguese.
- Working name: "GNOME Copilot" (`io.github.GnomeCopilot`). No logo exists yet.

## Evidence on Hand

- A screenshot of the GNOME Quick Settings menu, supplied by the user as the visual reference.
- Measured Needle 3 performance on the dev machine: ~0.3–0.4 s per request, ~106 MB peak RAM, 34/34 on the intent eval (PT/EN plus out-of-scope cases).
- No users, testimonials, or benchmarks beyond the above. Do not fabricate any.

## Product Principles

1. **Private by construction.** Nothing leaves the machine except pinned model downloads from Hugging Face.
2. **Native, not themed.** Reuse GNOME's own components and styles before drawing anything custom.
3. **Act, don't chat.** A request becomes an action and a one-line result, not a conversation.
4. **Ask before anything you can't undo.** Risk decides whether the assistant confirms first, not confidence alone.
5. **Out of the way.** It is invisible until summoned and gone after one outside click.

## Accessibility & Inclusion

Keyboard-first: the entry is focused when the panel opens, Enter sends, Esc closes, and cards are reachable with Tab. It respects GNOME's reduced-motion and high-contrast settings, and sizes text from the Shell's font scale.
