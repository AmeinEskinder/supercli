# Pairing a device

Devices (phone, tablet, second laptop) pair with the Host through a sealed
pairing handshake:

1. On the Host machine, run `unpeel init` (first run) or `unpeel pair`
   to display a pairing code and QR code.
2. On the device, open the Unpeel mobile app and scan the QR code (or enter
   the code manually).
3. The device and Host complete a cryptographic handshake; the device
   record is stored in `mobile/devices.json`.

Pairing codes are single-use and expire. If a code expires, generate a new
one — never reuse a code across devices.

To list paired devices:

```sh
unpeel devices list
```

To remove a device:

```sh
unpeel devices remove <device-id>
```

Removing a device revokes its access immediately; the Host stops accepting
its tokens.
