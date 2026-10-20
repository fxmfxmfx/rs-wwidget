# Temperatures widget

Reads Linux hwmon sensor files from `/sys/class/hwmon`, showing recognized CPU
and GPU temperatures and supported fan speeds. Edit `poll_seconds` in
`settings.toml` to change the interval. The optional `sysfs_root` setting can
point to a compatible alternate tree. Sensor names and channels vary by
hardware and kernel driver, so unsupported devices may not appear.
