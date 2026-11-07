//! Native iOS document picker for importing Java mod JARs from Files/iCloud.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSURL, ns_string};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerDelegate, UIDocumentPickerMode, UIDocumentPickerViewController,
};

#[derive(Debug)]
struct ModPickerIvars {
    mods_dir: PathBuf,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ModPickerIvars]
    struct ModPickerDelegate;

    unsafe impl NSObjectProtocol for ModPickerDelegate {}

    unsafe impl UIDocumentPickerDelegate for ModPickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick_documents_at_urls(
            &self,
            _controller: &UIDocumentPickerViewController,
            urls: &NSArray<NSURL>,
        ) {
            for index in 0..urls.count() {
                let url = urls.objectAtIndex(index);
                let started_access = unsafe { url.startAccessingSecurityScopedResource() };
                if let Some(path) = url.path() {
                    let source = PathBuf::from(path.to_string());
                    match crate::java_mods::import_jar_file(&source, &self.ivars().mods_dir) {
                        Ok(destination) => {
                            tracing::info!("Imported Java mod {}", destination.display())
                        }
                        Err(error) => tracing::warn!(
                            "Could not import selected mod {}: {error}",
                            source.display()
                        ),
                    }
                }
                if started_access {
                    unsafe { url.stopAccessingSecurityScopedResource() };
                }
            }
            // Keep the weakly referenced delegate alive through the end of
            // this callback; the next picker replaces the retained pair.
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn document_picker_was_cancelled(&self, _controller: &UIDocumentPickerViewController) {
            // Keep the weakly referenced delegate alive through the end of
            // this callback; the next picker replaces the retained pair.
        }
    }
);

impl ModPickerDelegate {
    fn new(mtm: MainThreadMarker, mods_dir: PathBuf) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ModPickerIvars { mods_dir });
        // SAFETY: NSObject's init has the expected signature for this subclass.
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    // UIDocumentPickerDelegate is weakly held by UIKit, so keep the picker and
    // delegate alive until the selection/cancel callback arrives.
    static ACTIVE_PICKER: RefCell<Option<(
        Retained<UIDocumentPickerViewController>,
        Retained<ModPickerDelegate>,
    )>> = const { RefCell::new(None) };
}

/// Present Apple's Files document picker and import selected `.jar` files.
pub fn open_mod_picker(mods_dir: &Path) -> Result<(), String> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "The iOS document picker must be opened on the main thread".to_owned())?;
    let application = UIApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    let window = application
        .keyWindow()
        .ok_or_else(|| "Pomme's iOS window is not available".to_owned())?;
    let presenter = window
        .rootViewController()
        .ok_or_else(|| "Pomme's root view controller is not available".to_owned())?;

    // `public.data` lets Files show JARs even when a provider does not publish
    // a dedicated JAR UTI. The Rust import helper strictly filters `.jar`.
    let document_types = NSArray::arrayWithObject(ns_string!("public.data"));
    #[allow(deprecated)]
    let picker = unsafe {
        UIDocumentPickerViewController::initWithDocumentTypes_inMode(
            UIDocumentPickerViewController::alloc(mtm),
            &document_types,
            UIDocumentPickerMode::Import,
        )
    };
    picker.setAllowsMultipleSelection(true);
    picker.setShouldShowFileExtensions(true);

    let delegate = ModPickerDelegate::new(mtm, mods_dir.to_path_buf());
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    ACTIVE_PICKER.with(|active| {
        *active.borrow_mut() = Some((picker.clone(), delegate));
    });

    unsafe { presenter.presentViewController_animated_completion(&picker, true, None) };
    Ok(())
}
