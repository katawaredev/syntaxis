use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::*;
use syntaxis_agent::{ImageAttachment, MAX_IMAGE_BYTES, MAX_PROMPT_IMAGES, MAX_TOTAL_IMAGE_BYTES};
use syntaxis_ui::prelude::{AppIcon, Icon};

#[component]
pub(super) fn ComposerAttachments(
    images: Vec<ImageAttachment>,
    on_remove: EventHandler<usize>,
) -> Element {
    rsx! {
        div { class: "flex gap-2 overflow-x-auto border-b border-border/70 px-3 pt-3 pb-2",
            for (index, image) in images.iter().enumerate() {
                AttachmentPreview {
                    key: "{index}-{image.name}",
                    image: image.clone(),
                    on_remove: move |()| on_remove.call(index),
                }
            }
        }
    }
}

#[component]
fn AttachmentPreview(image: ImageAttachment, on_remove: EventHandler<()>) -> Element {
    rsx! {
        div { class: "group relative size-18 shrink-0 overflow-hidden rounded-xl border border-border bg-background",
            img {
                class: "size-full object-cover",
                src: image.data_url(),
                alt: image.name.clone(),
            }
            button {
                class: "touch-only-visible absolute top-1 right-1 grid size-7 place-items-center rounded-full bg-background/90 text-foreground opacity-0 shadow transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
                aria_label: "Remove {image.name}",
                title: "Remove image",
                onclick: move |_| on_remove.call(()),
                Icon { icon: AppIcon::Close, size: 11 }
            }
            span { class: "absolute right-0 bottom-0 left-0 truncate bg-black/60 px-1.5 py-1 text-[8px] text-white",
                "{image.name}"
            }
        }
    }
}

pub(crate) async fn load_images(
    files: Vec<dioxus::html::FileData>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    for file in files {
        if attachments().len() >= MAX_PROMPT_IMAGES {
            error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
            break;
        }
        let mime_type = file.content_type().unwrap_or_default();
        if !mime_type.starts_with("image/") {
            error.set(Some(format!("{} is not an image.", file.name())));
            continue;
        }
        let total = attachments().iter().map(|image| image.size).sum::<u64>();
        if file.size() > MAX_IMAGE_BYTES
            || total.saturating_add(file.size()) > MAX_TOTAL_IMAGE_BYTES
        {
            error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
            continue;
        }
        match file.read_bytes().await {
            Ok(bytes) => attachments.write().push(ImageAttachment {
                name: file.name(),
                mime_type,
                size: file.size(),
                data: BASE64.encode(bytes),
            }),
            Err(_) => error.set(Some(format!("Could not read {}.", file.name()))),
        }
    }
}
