use crate::opfs::browser_error;
use js_sys::{Function, Promise};
use std::{cell::RefCell, rc::Rc};
use syntaxis_workspace::{WorkspaceError, WorkspaceResult};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransactionMode,
};
const DATABASE: &str = "syntaxis-guest";
const STORE: &str = "workspace-handles";
const LOCAL_ROOT_KEY: &str = "local-root";
pub(crate) async fn load_directory() -> WorkspaceResult<Option<FileSystemDirectoryHandle>> {
    let database = open_database().await?;
    let transaction = database
        .transaction_with_str(STORE)
        .map_err(|error| browser_error("Could not read saved folder access", error))?;
    let store = transaction
        .object_store(STORE)
        .map_err(|error| browser_error("Could not read saved folder access", error))?;
    let request = store
        .get(&JsValue::from_str(LOCAL_ROOT_KEY))
        .map_err(|error| browser_error("Could not read saved folder access", error))?;
    let value = await_request(request, "Could not read saved folder access").await?;
    if value.is_undefined() {
        Ok(None)
    } else {
        Ok(Some(value.unchecked_into()))
    }
}
pub(crate) async fn save_directory(directory: &FileSystemDirectoryHandle) -> WorkspaceResult<()> {
    let database = open_database().await?;
    let transaction = database
        .transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
        .map_err(|error| browser_error("Could not remember folder access", error))?;
    let store = transaction
        .object_store(STORE)
        .map_err(|error| browser_error("Could not remember folder access", error))?;
    let request = store
        .put_with_key(directory.as_ref(), &JsValue::from_str(LOCAL_ROOT_KEY))
        .map_err(|error| browser_error("Could not remember folder access", error))?;
    await_request(request, "Could not remember folder access").await?;
    Ok(())
}
async fn open_database() -> WorkspaceResult<IdbDatabase> {
    let window = web_sys::window().ok_or_else(WorkspaceError::internal)?;
    let factory = window
        .indexed_db()
        .map_err(|error| browser_error("IndexedDB is unavailable", error))?
        .ok_or_else(|| browser_error("IndexedDB is unavailable", JsValue::UNDEFINED))?;
    let request = factory
        .open_with_u32(DATABASE, 1)
        .map_err(|error| browser_error("Could not open browser settings", error))?;
    install_upgrade_handler(&request);
    let value = await_request(request.unchecked_into(), "Could not open browser settings").await?;
    Ok(value.unchecked_into())
}
fn install_upgrade_handler(request: &IdbOpenDbRequest) {
    let upgrade_request = request.clone();
    let cleanup_request = request.clone();
    let callback_slot = Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>));
    let callback_slot_for_handler = Rc::clone(&callback_slot);
    let callback = Closure::wrap(Box::new(move || {
        if let Ok(value) = upgrade_request.unchecked_ref::<IdbRequest>().result() {
            let database = value.unchecked_into::<IdbDatabase>();
            let _result = database.create_object_store(STORE);
        }
        cleanup_request.set_onupgradeneeded(None);
        let _ = callback_slot_for_handler.borrow_mut().take();
    }) as Box<dyn FnMut()>);
    *callback_slot.borrow_mut() = Some(callback);
    if let Some(callback) = callback_slot.borrow().as_ref() {
        request.set_onupgradeneeded(Some(callback.as_ref().unchecked_ref()));
    }
}
async fn await_request(request: IdbRequest, context: &str) -> WorkspaceResult<JsValue> {
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        let handlers = Rc::new(RequestHandlers::default());
        let success_request = request.clone();
        let success_request_handlers = request.clone();
        let success_resolve = resolve.clone();
        let success_reject = reject.clone();
        let success_handlers = Rc::clone(&handlers);
        let success = Closure::wrap(Box::new(move || {
            success_request_handlers.set_onsuccess(None);
            success_request_handlers.set_onerror(None);
            let _ = success_handlers.success.borrow_mut().take();
            let _ = success_handlers.failure.borrow_mut().take();
            match success_request.result() {
                Ok(value) => {
                    let _result = success_resolve.call1(&JsValue::UNDEFINED, &value);
                }
                Err(error) => {
                    let _result = success_reject.call1(&JsValue::UNDEFINED, &error);
                }
            }
        }) as Box<dyn FnMut()>);
        *handlers.success.borrow_mut() = Some(success);
        if let Some(success) = handlers.success.borrow().as_ref() {
            request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        }
        let failure_request = request.clone();
        let failure_handlers = Rc::clone(&handlers);
        let failure = Closure::wrap(Box::new(move || {
            failure_request.set_onsuccess(None);
            failure_request.set_onerror(None);
            let _ = failure_handlers.success.borrow_mut().take();
            let _ = failure_handlers.failure.borrow_mut().take();
            let _result = reject.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("IndexedDB request failed"),
            );
        }) as Box<dyn FnMut()>);
        *handlers.failure.borrow_mut() = Some(failure);
        if let Some(failure) = handlers.failure.borrow().as_ref() {
            request.set_onerror(Some(failure.as_ref().unchecked_ref()));
        }
    });
    JsFuture::from(promise)
        .await
        .map_err(|error| browser_error(context, error))
}
#[derive(Default)]
struct RequestHandlers {
    success: RefCell<Option<Closure<dyn FnMut()>>>,
    failure: RefCell<Option<Closure<dyn FnMut()>>>,
}
