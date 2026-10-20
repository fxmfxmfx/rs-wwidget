# Weather widget

By default, the provider estimates its location from the public IP address using
the HTTPS `ipwho.is` endpoint. It caches successful coordinates and city data
for 24 hours and retries failed lookups no more often than every 10 minutes.
The estimate may identify a nearby ISP or VPN exit location. To avoid IP
geolocation, set both `latitude` and `longitude` in `settings.toml`; those values
override auto-location.

When enabled, the widget sends a location request to ipwho.is only when the
coordinate cache is missing or expired, then requests current temperature,
apparent temperature and rain plus the next hourly precipitation probability
from Open-Meteo. Weather refreshes every 10 minutes; frames are emitted every
30 seconds. On request failures it keeps the last weather values for matching
coordinates and marks them stale. Requires `curl` and network access.
