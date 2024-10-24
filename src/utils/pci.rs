use std::{collections::BTreeMap, io::BufRead, sync::LazyLock, time::Instant};

use anyhow::{Context, Result};
use log::{debug, info, warn};

static VENDORS: LazyLock<BTreeMap<u16, Vendor>> = LazyLock::new(|| {
    init()
        .inspect_err(|e| warn!("Unable to parse pci.ids!\n{e}\n{}", e.backtrace()))
        .unwrap_or_default()
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

fn parse_pci_ids<R: BufRead>(reader: R) -> Result<BTreeMap<u16, Vendor>> {
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
                    .with_context(|| format!("this subdevice has no pid (line: {line})"))?,
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

fn init() -> Result<BTreeMap<u16, Vendor>> {
    debug!("Parsing pci.ids…");

    let start = Instant::now();

    // first check if we can use flatpak's FS to get to the (probably newer) host's pci.ids file
    //
    // if that doesn't work, we're either not on flatpak or we're not allowed to see the host's pci.ids for some reason,
    // so try to either access flatpak's own (probably older) pci.ids or the host's if we're not on flatpak
    let file = std::fs::File::open("/run/host/usr/share/hwdata/pci.ids")
        .or_else(|_| std::fs::File::open("/usr/share/hwdata/pci.ids"))?;

    let reader = std::io::BufReader::new(file);

    let map = parse_pci_ids(reader)?;

    let vendors_count = map.len();
    let devices_count: usize = map.values().map(|vendor| vendor.devices.len()).sum();
    let subdevices_count: usize = map
        .values()
        .map(|vendor| {
            vendor
                .devices
                .values()
                .map(|device| device.sub_devices.len())
                .sum::<usize>()
        })
        .sum();

    let elapsed = start.elapsed();

    info!("Successfully parsed pci.ids within {elapsed:.2?} (vendors: {vendors_count}, devices: {devices_count}, subdevices: {subdevices_count})");

    Ok(map)
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeMap, io::BufReader};

    use crate::utils::pci::{parse_pci_ids, Device, Subdevice, Vendor};

    #[test]
    fn valid_empty() {
        let pci_ids = "";

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader).unwrap();

        let expected = BTreeMap::new();

        assert_eq!(expected, result);
    }

    #[test]
    fn valid_empty_comment() {
        let pci_ids = "# just a comment";

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader).unwrap();

        let expected = BTreeMap::new();

        assert_eq!(expected, result);
    }

    #[test]
    fn valid_empty_class() {
        let pci_ids = "C 00 Unclassified device";

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader).unwrap();

        let expected = BTreeMap::new();

        assert_eq!(expected, result);
    }

    #[test]
    fn valid_single_vendor() {
        let pci_ids = "1234  Example Technologies Inc.";

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader).unwrap();

        let expected = BTreeMap::from([(
            0x1234,
            Vendor {
                id: 0x1234,
                name: "Example Technologies Inc.".into(),
                devices: BTreeMap::new(),
            },
        )]);

        assert_eq!(expected, result);
    }

    #[test]
    fn valid_single_device() {
        let pci_ids = concat!(
            "1234  Example Technologies Inc.\n",
            "\t5678  Super Device 3000"
        );

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader).unwrap();

        let expected = BTreeMap::from([(
            0x1234,
            Vendor {
                id: 0x1234,
                name: "Example Technologies Inc.".into(),
                devices: BTreeMap::from([(
                    0x5678,
                    Device {
                        id: 0x5678,
                        vendor_id: 0x1234,
                        name: "Super Device 3000".into(),
                        sub_devices: vec![],
                    },
                )]),
            },
        )]);

        assert_eq!(expected, result);
    }

    #[test]
    fn valid_complex() {
        let pci_ids = concat!(
            "# interesting comment\n",
            "\n",
            "1234  Example Technologies Inc.\n",
            "# another interesting comment\n",
            "\t5678  Super Device 3000\n",
            "\t5679  Super Device 3000.2 Gen 2x2 5Gbps Somewhat Hi-Speed\n",
            "dead  Zombie Computers LLC\n",
            "\tbeef  Brain\n",
            "\t\tdead cafe  Energy Depot\n",
            "\t\t1234 abcd  Example Braincell\n",
            "# most interesting comment yet\n",
            "C 00 Unclassified device"
        );

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader).unwrap();

        let expected = BTreeMap::from([
            (
                0x1234,
                Vendor {
                    id: 0x1234,
                    name: "Example Technologies Inc.".into(),
                    devices: BTreeMap::from([
                        (
                            0x5678,
                            Device {
                                id: 0x5678,
                                vendor_id: 0x1234,
                                name: "Super Device 3000".into(),
                                sub_devices: vec![],
                            },
                        ),
                        (
                            0x5679,
                            Device {
                                id: 0x5679,
                                vendor_id: 0x1234,
                                name: "Super Device 3000.2 Gen 2x2 5Gbps Somewhat Hi-Speed".into(),
                                sub_devices: vec![],
                            },
                        ),
                    ]),
                },
            ),
            (
                0xdead,
                Vendor {
                    id: 0xdead,
                    name: "Zombie Computers LLC".into(),
                    devices: BTreeMap::from([(
                        0xbeef,
                        Device {
                            id: 0xbeef,
                            vendor_id: 0xdead,
                            name: "Brain".into(),
                            sub_devices: vec![
                                Subdevice {
                                    id: 0xcafe,
                                    vendor_id: 0xdead,
                                    name: "Energy Depot".into(),
                                },
                                Subdevice {
                                    id: 0xabcd,
                                    vendor_id: 0x1234,
                                    name: "Example Braincell".into(),
                                },
                            ],
                        },
                    )]),
                },
            ),
        ]);

        assert_eq!(expected, result)
    }

    #[test]
    fn invalid_no_preceding_vendor() {
        let pci_ids = "\tabcd  Vendorless Device";

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader);

        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn invalid_no_preceding_device() {
        let pci_ids = "\t\t0123 abcd  Vendorless Device";

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader);

        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn invalid_malformed_vendor() {
        let pci_ids = concat!("Vendor with no ID :(\n", "\t1234 Device");

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader);

        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn invalid_malformed_device() {
        let pci_ids = concat!("0123  Vendor\n", "\tNo device ID :(");

        let reader = BufReader::new(pci_ids.as_bytes());

        let result = parse_pci_ids(reader);

        assert_eq!(result.is_err(), true);
    }
}
