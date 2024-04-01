use std::{collections::BTreeMap, io::BufRead};

use anyhow::{Context, Result};
use log::{debug, warn};
use once_cell::sync::Lazy;

static VENDORS: Lazy<BTreeMap<u16, Vendor>> = Lazy::new(|| {
    let res = parse_pci_ids();

    if let Err(error) = res.as_ref() {
        warn!("Unable to read pci.ids, reason: {error}")
    } else {
        debug!("Successfully parsed pci.ids")
    }

    res.unwrap_or_default()
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subdevice {
    id: u16,
    vendor_id: u16,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    id: u16,
    vendor_id: u16,
    name: String,
    sub_devices: Vec<Subdevice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vendor {
    id: u16,
    name: String,
    devices: BTreeMap<u16, Device>,
}

impl Device {
    pub fn subdevices(&'static self) -> impl Iterator<Item = &'static Subdevice> {
        self.sub_devices.iter()
    }

    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<&'static Self> {
        VENDORS.get(&vid).and_then(|vendor| vendor.get_device(pid))
    }

    pub fn vendor(&self) -> &'static Vendor {
        VENDORS
            .get(&self.vendor_id)
            .expect("device with no vendor?")
    }

    pub fn name(&'static self) -> &'static str {
        &self.name
    }

    pub fn pid(&self) -> u16 {
        self.id
    }
}

impl Vendor {
    pub fn from_vid(vid: u16) -> Option<&'static Vendor> {
        VENDORS.get(&vid)
    }

    pub fn devices(&'static self) -> impl Iterator<Item = &'static Device> {
        self.devices.values()
    }

    pub fn get_device(&'static self, pid: u16) -> Option<&'static Device> {
        self.devices.get(&pid)
    }

    pub fn name(&'static self) -> &'static str {
        &self.name
    }

    pub fn vid(&self) -> u16 {
        self.id
    }
}

fn parse_pci_ids() -> Result<BTreeMap<u16, Vendor>> {
    // first check if we can use flatpak's FS to get to the (probably newer) host's pci.ids file
    //
    // if that doesn't work, we're either not on flatpak or we're not allowed to see the host's pci.ids for some reason,
    // so try to either access flatpak's own (probably older) pci.ids or the host's if we're not on flatpak
    let file = std::fs::File::open("/run/host/usr/share/hwdata/pci.ids")
        .or_else(|_| std::fs::File::open("/usr/share/hwdata/pci.ids"))?;

    debug!("Parsing pci.ids…");

    let reader = std::io::BufReader::new(file);

    let mut seen: BTreeMap<u16, Vendor> = BTreeMap::new();

    for line in reader.lines().map_while(Result::ok) {
        if line.starts_with('C') {
            // case 1: we've reached the classes, time to stop
            break;
        } else if line.starts_with('#') || line.is_empty() {
            // case 2: we're seeing a comment, don't care
            // case 3: we're seeing an empty line, also don't care
            continue;
        } else if line.starts_with("\t\t") {
            // case 4: we're seeing a new sub device of the last seen device
            let mut split = line.trim_start().splitn(4, ' ');

            let sub_vid = u16::from_str_radix(
                split
                    .next()
                    .with_context(|| format!("this subdevice has no vid (line: {line})"))?,
                16,
            )?;

            let sub_pid = u16::from_str_radix(
                split
                    .next()
                    .with_context(|| format!("this subdevice has no üid (line: {line})"))?,
                16,
            )?;

            let name = split
                .last()
                .map(str::to_string)
                .with_context(|| format!("this vendor has no name (line: {line})"))?;

            let subdevice = Subdevice {
                id: sub_pid,
                vendor_id: sub_vid,
                name,
            };

            seen.values_mut()
                .last()
                .and_then(|vendor| vendor.devices.values_mut().last())
                .with_context(|| format!("no preceding vendor (line: {line})"))?
                .sub_devices
                .push(subdevice);
        } else if line.starts_with('\t') {
            // case 5: we're seeing a new device of the last seen vendor
            let mut split = line.trim_start().split("  ");

            let vid = *seen
                .keys()
                .last()
                .with_context(|| format!("no preceding device (line: {line})"))?;

            let pid = u16::from_str_radix(
                split
                    .next()
                    .with_context(|| format!("this device has no pid (line: {line})"))?,
                16,
            )?;

            let name = split
                .next()
                .map(str::to_string)
                .with_context(|| format!("this vendor has no name (line: {line})"))?;

            let device = Device {
                id: pid,
                vendor_id: vid,
                name,
                sub_devices: Vec::new(),
            };

            seen.values_mut()
                .last()
                .with_context(|| format!("no preceding device (line: {line})"))?
                .devices
                .insert(pid, device);
        } else {
            // case 6: we're seeing a new vendor
            let mut split = line.split("  ");

            let vid = u16::from_str_radix(
                split
                    .next()
                    .with_context(|| format!("this vendor has no vid (line: {line})"))?,
                16,
            )?;

            let name = split
                .next()
                .map(str::to_string)
                .with_context(|| format!("this vendor has no name (line: {line})"))?;

            let vendor = Vendor {
                id: vid,
                name,
                devices: BTreeMap::new(),
            };

            seen.insert(vid, vendor);
        }
    }

    Ok(seen)
}
