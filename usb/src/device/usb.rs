use crate::commands::Command;
use crate::device::base::{
    AttachGoXLR, ExecutableGoXLR, FullGoXLRDevice, GoXLRCommands, GoXLRDevice,
};
use crate::goxlr::{PID_GOXLR_FULL, PID_GOXLR_MINI, VID_GOXLR};
use anyhow::{bail, Error, Result};
use byteorder::{ByteOrder, LittleEndian};
use log::{debug, error};
use rusb::Error::Pipe;
use rusb::{
    Device, DeviceDescriptor, DeviceHandle, Direction, GlobalContext, Recipient, RequestType,
};
use std::thread::sleep;
use std::time::Duration;

pub struct GoXLRUSB {
    handle: DeviceHandle<GlobalContext>,
    device: Device<GlobalContext>,
    descriptor: DeviceDescriptor,

    command_count: u16,
    timeout: Duration,
}

impl GoXLRUSB {
    fn find_device(device: GoXLRDevice) -> Result<(Device<GlobalContext>, DeviceDescriptor)> {
        if let Ok(devices) = rusb::devices() {
            for usb_device in devices.iter() {
                if usb_device.bus_number() == device.bus_number
                    && usb_device.address() == device.address
                {
                    if let Ok(descriptor) = usb_device.device_descriptor() {
                        return Ok((usb_device, descriptor));
                    }
                }
            }
        }
        bail!("Specified Device not Found!")
    }

    pub fn write_control(
        &mut self,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<(), rusb::Error> {
        self.handle.write_control(
            rusb::request_type(Direction::Out, RequestType::Vendor, Recipient::Interface),
            request,
            value,
            index,
            data,
            self.timeout,
        )?;

        Ok(())
    }

    pub fn read_control(
        &mut self,
        request: u8,
        value: u16,
        index: u16,
        length: usize,
    ) -> Result<Vec<u8>, rusb::Error> {
        let mut buf = vec![0; length];
        let response_length = self.handle.read_control(
            rusb::request_type(Direction::In, RequestType::Vendor, Recipient::Interface),
            request,
            value,
            index,
            &mut buf,
            self.timeout,
        )?;
        buf.truncate(response_length);
        Ok(buf)
    }
}

impl AttachGoXLR for GoXLRUSB {
    fn from_device(device: GoXLRDevice) -> Result<Self> {
        // Firstly, we need to locate the USB device based on the location..
        let (device, descriptor) = GoXLRUSB::find_device(device)?;
        let handle = device.open()?;

        Ok(Self {
            device: handle.device(),
            handle,
            descriptor,
            command_count: 0,
            timeout: Duration::from_secs(1),
        })
    }
}

impl ExecutableGoXLR for GoXLRUSB {
    fn perform_request(&mut self, command: Command, body: &[u8], retry: bool) -> Result<Vec<u8>> {
        if command == Command::ResetCommandIndex {
            self.command_count = 0;
        } else {
            if self.command_count == u16::MAX {
                let _ = self.request_data(Command::ResetCommandIndex, &[])?;
            }
            self.command_count += 1;
        }

        let command_index = self.command_count;
        let mut full_request = vec![0; 16];
        LittleEndian::write_u32(&mut full_request[0..4], command.command_id());
        LittleEndian::write_u16(&mut full_request[4..6], body.len() as u16);
        LittleEndian::write_u16(&mut full_request[6..8], command_index);
        full_request.extend(body);

        self.write_control(2, 0, 0, &full_request)?;

        // The full fat GoXLR can handle requests incredibly quickly..
        let mut sleep_time = Duration::from_millis(3);
        if self.descriptor.product_id() == PID_GOXLR_MINI {
            // The mini, however, cannot.
            sleep_time = Duration::from_millis(10);
        }
        sleep(sleep_time);

        // Interrupt reading doesnt work, because we can't claim the interface.
        //self.await_interrupt(Duration::from_secs(2));

        let mut response = vec![];

        for i in 0..20 {
            let response_value = self.read_control(3, 0, 0, 1040);
            if response_value == Err(Pipe) {
                if i < 20 {
                    debug!("Response not arrived yet for {:?}, sleeping and retrying (Attempt {} of 20)", command, i + 1);
                    sleep(sleep_time);
                    continue;
                } else {
                    debug!("Failed to receive response (Attempt 20 of 20), possible Dead GoXLR?");
                    return Err(Error::from(response_value.err().unwrap()));
                }
            }
            if response_value.is_err() {
                let err = response_value.err().unwrap();
                debug!("Error Occurred during packet read: {}", err);
                return Err(Error::from(err));
            }

            let mut response_header = response_value.unwrap();
            if response_header.len() < 16 {
                error!(
                    "Invalid Response received from the GoXLR, Expected: 16, Received: {}",
                    response_header.len()
                );
                return Err(Error::from(Pipe));
            }

            response = response_header.split_off(16);
            let response_length = LittleEndian::read_u16(&response_header[4..6]);
            let response_command_index = LittleEndian::read_u16(&response_header[6..8]);

            if response_command_index != command_index {
                debug!("Mismatched Command Indexes..");
                debug!(
                    "Expected {}, received: {}",
                    command_index, response_command_index
                );
                debug!("Full Request: {:?}", full_request);
                debug!("Response Header: {:?}", response_header);
                debug!("Response Body: {:?}", response);

                return if !retry {
                    debug!("Attempting Resync and Retry");
                    let _ = self.perform_request(Command::ResetCommandIndex, &[], true)?;

                    debug!("Resync complete, retrying Command..");
                    self.perform_request(command, body, true)
                } else {
                    debug!("Resync Failed, Throwing Error..");
                    Err(Error::from(rusb::Error::Other))
                };
            }

            debug_assert!(response.len() == response_length as usize);
            break;
        }

        Ok(response)
    }
}

impl GoXLRCommands for GoXLRUSB {}
impl FullGoXLRDevice for GoXLRUSB {}

pub fn find_devices() -> Vec<GoXLRDevice> {
    let mut found_devices: Vec<GoXLRDevice> = Vec::new();

    if let Ok(devices) = rusb::devices() {
        for device in devices.iter() {
            if let Ok(descriptor) = device.device_descriptor() {
                let bus_number = device.bus_number();
                let address = device.address();

                if descriptor.vendor_id() == VID_GOXLR
                    && (descriptor.product_id() == PID_GOXLR_FULL
                        || descriptor.product_id() == PID_GOXLR_MINI)
                {
                    found_devices.push(GoXLRDevice {
                        bus_number,
                        address,
                    });
                }
            }
        }
    }

    found_devices
}
