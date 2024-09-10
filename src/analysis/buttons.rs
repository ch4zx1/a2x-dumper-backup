use std::collections::BTreeMap;

use anyhow::{bail, Result};

use log::debug;

use memflow::prelude::v1::*;

use pelite::pattern;
use pelite::pe64::{Pe, PeView};

use crate::source2::KeyButton;

pub type ButtonMap = BTreeMap<String, imem>;

pub fn buttons(process: &mut IntoProcessInstanceArcBox<'_>) -> Result<ButtonMap> {
    let module = process.module_by_name("client.dll")?;

    let buf = process
        .read_raw(module.base, module.size as _)
        .data_part()?;

    let view = PeView::from_bytes(&buf)?;

    let mut save = [0; 2];

    if !view
        .scanner()
        .finds_code(pattern!("488b15${'} 4885d2 74? 0f1f40"), &mut save)
    {
        bail!("outdated button list pattern");
    }

    read_buttons(process, &module, module.base + save[1])
}

fn read_buttons(
    process: &mut IntoProcessInstanceArcBox<'_>,
    module: &ModuleInfo,
    list_addr: Address,
) -> Result<ButtonMap> {
    let mut map = ButtonMap::new();

    let mut cur_button = Pointer64::<KeyButton>::from(process.read_addr64(list_addr).data_part()?);

    while !cur_button.is_null() {
        let button = process.read_ptr(cur_button).data_part()?;
        let name = process.read_utf8(button.name.address(), 32).data_part()?;
        let rva = (cur_button.address() - module.base) + offset_of!(KeyButton.state) as imem;

        debug!(
            "found button: {} at {:#X} ({} + {:#X})",
            name,
            cur_button.to_umem() + offset_of!(KeyButton.state) as umem,
            module.name,
            rva
        );

        map.insert(name, rva);

        cur_button = button.next;
    }

    Ok(map)
}
