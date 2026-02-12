"""Constants for the Desktop App integration."""

DOMAIN = "desktop_app"

# Storage
STORAGE_KEY = "desktop_app_registrations"
STORAGE_VERSION = 1

# Data keys
DATA_CONFIG_ENTRIES = "config_entries"
DATA_DEVICES = "devices"
DATA_DELETED_IDS = "deleted_ids"
DATA_PENDING_UPDATES = "pending_updates"
DATA_STORE = "store"
DATA_API_VIEW_REGISTERED = "api_view_registered"
DATA_BINARY_SENSOR = "binary_sensor"
DATA_SENSOR = "sensor"

# Device attributes
ATTR_DEVICE_ID = "device_id"
ATTR_DEVICE_NAME = "device_name"
ATTR_MANUFACTURER = "manufacturer"
ATTR_MODEL = "model"
ATTR_OS_NAME = "os_name"
ATTR_OS_VERSION = "os_version"
ATTR_APP_VERSION = "app_version"

# Webhook
ATTR_WEBHOOK_ID = "webhook_id"

# Sensor attributes
ATTR_SENSOR_TYPE = "sensor_type"
ATTR_SENSOR_NAME = "sensor_name"
ATTR_SENSOR_UNIQUE_ID = "sensor_unique_id"
ATTR_SENSOR_STATE = "sensor_state"
ATTR_SENSOR_ICON = "sensor_icon"
ATTR_SENSOR_ATTRIBUTES = "sensor_attributes"
ATTR_SENSOR_DEVICE_CLASS = "sensor_device_class"
ATTR_SENSOR_UNIT_OF_MEASUREMENT = "sensor_unit_of_measurement"
ATTR_SENSOR_STATE_CLASS = "sensor_state_class"
ATTR_SENSOR_ENTITY_CATEGORY = "sensor_entity_category"

# Webhook command types
COMMAND_REGISTER_SENSOR = "register_sensor"
COMMAND_UPDATE_SENSOR_STATES = "update_sensor_states"
COMMAND_UPDATE_REGISTRATION = "update_registration"

# Signal templates
SIGNAL_SENSOR_UPDATE = f"{DOMAIN}_sensor_update_{{}}_{{}}"
SIGNAL_SENSOR_REGISTER = f"{DOMAIN}_sensor_register_{{}}_{{}}"

# Platforms
PLATFORMS = ["sensor", "binary_sensor"]
