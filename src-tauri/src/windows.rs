use std::path::Path;

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IPropertyStore, PROPERTYKEY, PSGetPropertyKeyFromName,
};
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
};
use windows::core::{BSTR, HSTRING, Interface, PROPVARIANT, PWSTR, w};

pub fn reattach_console() {
    // safety: FFI
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn update_jump_list(recent: &mut Vec<String>) -> Result<()> {
    // create a jump list
    // safety: FFI
    let jump_list: ICustomDestinationList =
        unsafe { CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)? };

    // initialise the list and honour removals requested by the user
    let mut max_destinations = 0u32;
    let mut destination_path = vec![0u16; MAX_PATH as usize];

    // safety: GetArguments() calls len() on the provided slice, and produces a null-terminated string for PWSTR::to_string()
    unsafe {
        let removed_destinations: IObjectArray = jump_list.BeginList(&mut max_destinations)?;
        for i in 0..removed_destinations.GetCount()? {
            let removed_link: IShellLinkW = removed_destinations.GetAt(i)?; // safety: i <= GetCount()
            removed_link.GetArguments(&mut destination_path)?;
            let removed_path_wstr = PWSTR::from_raw(destination_path.as_mut_ptr());
            if !removed_path_wstr.is_null() {
                let removed_path = removed_path_wstr.to_string()?;
                recent.retain(|x| *x != removed_path);
            }
        }
    };

    // safety: FFI
    let items: IObjectCollection =
        unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)? };

    // turn the paths into IShellLinks
    for path in recent {
        let path_wstr: HSTRING = (&*path).into();
        let exe_wstr: HSTRING = std::env::current_exe()?.as_os_str().into();
        let dir_wstr: HSTRING = Path::new(path)
            .file_name()
            .ok_or(anyhow!("repo path is not a directory"))?
            .into();
        let dir_wstr = BSTR::from_wide(dir_wstr.as_wide())?;

        // safety: FFI
        unsafe {
            let link = create_directory_link(exe_wstr, path_wstr, dir_wstr)?;
            items.AddObject(&link)?;
        }
    }

    // add a custom category
    // safety: FFI
    unsafe {
        let array: IObjectArray = items.cast()?;
        jump_list.AppendCategory(w!("Recent"), &array)?;
        jump_list.CommitList()?;
    }

    Ok(())
}

// safety: no invariants, it's all FFI
unsafe fn create_directory_link(path: HSTRING, args: HSTRING, title: BSTR) -> Result<IShellLinkW> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&path)?; // launch ourselves...
        link.SetIconLocation(w!("%SystemRoot%\\System32\\shell32.dll"), 3)?; // ...with the icon for a directory...
        link.SetArguments(&args)?; // ...the directory as an argument...
        link.SetDescription(&args)?; // ...and a tooltip containing just the directory name

        // the actual display string must be set as a property because IShellLink is primarily for shortcuts
        let title_value = PROPVARIANT::from(title);
        let mut title_key = PROPERTYKEY::default();
        PSGetPropertyKeyFromName(w!("System.Title"), &mut title_key)?;

        let store: IPropertyStore = link.cast()?;
        store.SetValue(&title_key, &title_value)?;
        store.Commit()?;

        Ok(link)
    }
}
