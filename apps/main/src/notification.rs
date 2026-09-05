use dioxus::prelude::document;

/// A platform notification independent of the application's notification model.
pub(crate) struct SystemNotification {
    pub title: String,
    pub body: String,
    pub route: String,
    pub tag: String,
}

/// Shows a notification using the implementation available to the current UI target.
pub(crate) fn show(notification: SystemNotification) {
    let eval = document::eval(
        r#"
        const [title, body, route, tag] = await dioxus.recv();
        if (!("Notification" in globalThis) || Notification.permission !== "granted") return;
        const alert = new Notification(title, { body, tag });
        alert.onclick = () => {
            globalThis.focus();
            globalThis.location.href = route;
            alert.close();
        };
        "#,
    );
    let _ = eval.send((
        notification.title,
        notification.body,
        notification.route,
        notification.tag,
    ));
}
