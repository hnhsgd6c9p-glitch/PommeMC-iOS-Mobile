//! Native iOS document pickers for importing Java mods and resource packs from
//! Files.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSURL, ns_string};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerDelegate, UIDocumentPickerMode, UIDocumentPickerViewController,
};

#[derive(Clone, Copy, Debug)]
enum ImportKind {
    Mod,
    ResourcePack,
}

#[derive(Debug)]
struct PickerIvars {
    destination_dir: PathBuf,
    kind: ImportKind,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PickerIvars]
    struct FileImportPickerDelegate;

    unsafe impl NSObjectProtocol for FileImportPickerDelegate {}

    unsafe impl UIDocumentPickerDelegate for FileImportPickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick_documents_at_urls(
            &self,
            _controller: &UIDocumentPickerViewController,
            urls: &NSArray<NSURL>,
        ) {
            let mut imported_resource_pack = false;
            for index in 0..urls.count() {
                let url = urls.objectAtIndex(index);
                let started_access = unsafe { url.startAccessingSecurityScopedResource() };
                if let Some(path) = url.path() {
                    let source = PathBuf::from(path.to_string());
                    match self.ivars().kind {
                        ImportKind::Mod => {
                            match crate::java_mods::import_jar_file(
                                &source,
                                &self.ivars().destination_dir,
                            ) {
                                Ok(destination) => {
                                    tracing::info!("Imported Java mod {}", destination.display())
                                }
                                Err(error) => tracing::warn!(
                                    "Could not import selected mod {}: {error}",
                                    source.display()
                                ),
                            }
                        }
                        ImportKind::ResourcePack => {
                            match crate::resource_pack::import_local_pack_file(
                                &source,
                                &self.ivars().destination_dir,
                            ) {
                                Ok(destination) => {
                                    tracing::info!(
                                        "Imported resource pack {}",
                                        destination.display()
                                    );
                                    imported_resource_pack = true;
                                }
                                Err(error) => tracing::warn!(
                                    "Could not import resource pack {}: {error}",
                                    source.display()
                                ),
                            }
                        }
                    }
                }
                if started_access {
                    unsafe { url.stopAccessingSecurityScopedResource() };
                }
            }
            if imported_resource_pack {
                RESOURCE_PACK_IMPORT_COMPLETED.with(|completed| completed.set(true));
            }
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn document_picker_was_cancelled(&self, _controller: &UIDocumentPickerViewController) {
            // UIKit releases the active picker/delegate pair after this
            // callback.
        }
    }
);

impl FileImportPickerDelegate {
    fn new(mtm: MainThreadMarker, destination_dir: PathBuf, kind: ImportKind) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PickerIvars {
            destination_dir,
            kind,
        });
        // SAFETY: NSObject's init has the expected signature for this subclass.
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    // UIDocumentPickerDelegate is weakly held by UIKit, so retain both objects
    // until the selection/cancel callback arrives.
    static ACTIVE_PICKER: RefCell<Option<(
        Retained<UIDocumentPickerViewController>,
        Retained<FileImportPickerDelegate>,
    )>> = const { RefCell::new(None) };
    static RESOURCE_PACK_IMPORT_COMPLETED: Cell<bool> = const { Cell::new(false) };
}

/// Present Apple's Files picker and import selected .jar files.
pub fn open_mod_picker(mods_dir: &Path) -> Result<(), String> {
    open_picker(mods_dir, ImportKind::Mod)
}

/// Present Apple's Files picker and import selected resource-pack .zip files.
pub fn open_resource_pack_picker(packs_dir: &Path) -> Result<(), String> {
    open_picker(packs_dir, ImportKind::ResourcePack)
}

/// Called by the menu loop after the asynchronous Files picker callback.
pub fn take_resource_pack_import_completed() -> bool {
    RESOURCE_PACK_IMPORT_COMPLETED.with(|completed| completed.replace(false))
}

fn open_picker(destination_dir: &Path, kind: ImportKind) -> Result<(), String> {
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

    // public.data keeps Files providers that omit a dedicated JAR/ZIP UTI
    // selectable; the destination-specific import helper validates extensions
    // and archive contents before copying.
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

    let delegate = FileImportPickerDelegate::new(mtm, destination_dir.to_path_buf(), kind);
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    ACTIVE_PICKER.with(|active| {
        *active.borrow_mut() = Some((picker.clone(), delegate));
    });

    unsafe { presenter.presentViewController_animated_completion(&picker, true, None) };
    Ok(())
}
