# Syntaxis UI

Shared, application-owned Dioxus 0.7 UI components for Syntaxis.

This crate owns accessible visual primitives and stable application chrome. It
does not depend on routing, workspace data, editor buffers, terminal sessions,
or Git operations. Feature crates should compose these components while keeping
their domain state and actions local.

Components are organized by responsibility:

- buttons, icons, badges, menus, dialogs, drawers, toasts, and empty states;
- text inputs, text areas, native selects, and a Dioxus-primitive checkbox;
- a shared small, medium, and large control-size scale for buttons, triggers, and
  form controls;
- fields that connect labels, descriptions, errors, and required state to their
  nested controls;
- dialog form scaffolding and destructive-operation notices;
- shared panel headers, tab lists, and closable panel tabs.

The components use semantic Tailwind classes such as `bg-background` and
`text-muted-foreground`. Applications consuming this crate must include its Rust
sources in Tailwind content discovery and provide the corresponding theme
tokens.

`Field` supplies the control id and accessibility metadata to a nested form
control, so feature code does not repeat those attributes:

```rust
rsx! {
    Field {
        control_id: "repository-url",
        label: "Repository URL",
        description: "HTTPS and SSH URLs are supported.",
        required: true,
        TextInput {
            input_type: TextInputType::Url,
            placeholder: "https://github.com/owner/repository.git",
            oninput: move |event: FormEvent| url.set(event.value()),
        }
    }
}
```
