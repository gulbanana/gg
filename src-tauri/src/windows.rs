use std::path::Path;

use anyhow::{anyhow, Result};
use windows::core::{w, Interface, BSTR, HSTRING, PROPVARIANT, PWSTR};
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IPropertyStore, PSGetPropertyKeyFromName, PROPERTYKEY,
};
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
};

pub fn reattach_console() {
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[allow(dead_code)]
pub fn update_jump_list(recent: &mut Vec<String>, path: &String) -> Result<()> {
    unsafe {
        // init
        let mut min_slots = 0u32;
        let jump_list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        let removed_destinations: IObjectArray = jump_list.BeginList(&mut min_slots)?;

        // check for removals by the user
        let mut destination_args = vec![0u16; MAX_PATH as usize];
        for i in 0..removed_destinations.GetCount()? {
            let removed_link: IShellLinkW = removed_destinations.GetAt(i)?;
            removed_link.GetArguments(&mut destination_args)?;
            let removed_path = PWSTR::from_raw(destination_args.as_mut_ptr()).to_string()?;
            recent.retain(|x| *x != removed_path);
        }

        // add the new path as most-recent and trim to the configured max size
        recent.retain(|x| x != path);
        recent.insert(0, path.to_owned());

        // turn the paths into IShellLinks
        let items: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for path in recent {
            let link = create_directory_link(path)?;
            items.AddObject(&link)?;
        }

        // create a single-category jump list
        let array: IObjectArray = items.cast()?;
        jump_list.AppendCategory(w!("Recent"), &array)?;
        jump_list.CommitList()?;
    }

    Ok(())
}

unsafe fn create_directory_link(path: &String) -> Result<IShellLinkW> {
    let exe_wstr: HSTRING = std::env::current_exe()?.as_os_str().into();
    let path_wstr: HSTRING = path.into();
    let dir_wstr: HSTRING = Path::new(path)
        .file_name()
        .ok_or(anyhow!("repo path is not a directory"))?
        .into();

    let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
    link.SetPath(&exe_wstr)?; // launch ourselves...
    link.SetIconLocation(w!("%SystemRoot%\\System32\\shell32.dll"), 3)?; // ...with the icon for a directory...
    link.SetArguments(&path_wstr)?; // ...the directory as an argument...
    link.SetDescription(&path_wstr)?; // ...and a tooltip containing just the directory name

    // the actual display string must be set as a property because IShellLink is primarily for shortcuts
    let title_value = PROPVARIANT::from(BSTR::from_wide(dir_wstr.as_wide())?);
    let mut title_key = PROPERTYKEY::default();
    PSGetPropertyKeyFromName(w!("System.Title"), &mut title_key)?;

    let store: IPropertyStore = link.cast()?;
    store.SetValue(&title_key, &title_value)?;
    store.Commit()?;

    Ok(link)
}
