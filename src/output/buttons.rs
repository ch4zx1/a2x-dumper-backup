use std::collections::BTreeMap;
use std::fmt::{self, Write};

use super::{ButtonMap, CodeWriter, Formatter};

impl CodeWriter for ButtonMap {
    fn write_cs(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        fmt.block("namespace CS2Dumper", false, |fmt| {
            writeln!(fmt, "// Module: client.dll")?;

            fmt.block("public static class Buttons", false, |fmt| {
                for (name, value) in self {
                    writeln!(fmt, "public const nint {} = {:#X};", name, value)?;
                }

                Ok(())
            })
        })
    }

    fn write_hpp(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "#pragma once\n")?;
        writeln!(fmt, "#include <cstddef>\n")?;

        fmt.block("namespace cs2_dumper", false, |fmt| {
            writeln!(fmt, "// Module: client.dll")?;

            fmt.block("namespace buttons", false, |fmt| {
                for (name, value) in self {
                    writeln!(fmt, "constexpr std::ptrdiff_t {} = {:#X};", name, value)?;
                }

                Ok(())
            })
        })
    }

    fn write_json(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        let content = {
            let buttons: BTreeMap<_, _> = self.iter().map(|(name, value)| (name, value)).collect();

            BTreeMap::from_iter([("client.dll", buttons)])
        };

        fmt.write_str(&serde_json::to_string_pretty(&content).unwrap())
    }

    fn write_rs(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "#![allow(non_upper_case_globals, unused)]\n")?;

        fmt.block("pub mod cs2_dumper", false, |fmt| {
            writeln!(fmt, "// Module: client.dll")?;

            fmt.block("pub mod buttons", false, |fmt| {
                for (name, value) in self {
                    let mut name = name.clone();

                    if name == "use" {
                        name = format!("r#{}", name);
                    }

                    writeln!(fmt, "pub const {}: usize = {:#X};", name, value)?;
                }

                Ok(())
            })
        })
    }
}
