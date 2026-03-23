use crate::util::hasher::Hasher;
use alloc::string::ToString;
use common::models::{SystemConfiguration, WidgetInstallationData};
use defmt::info;
use esp_bootloader_esp_idf::partitions;
use esp_hal::peripherals::FLASH;
use esp_hal::peripherals::SHA;
use esp_nvs::{Key, Nvs, error::Error as NvsError};
use esp_storage::{FlashStorage, FlashStorageError};

pub struct Storage<'d> {
    nvs: Nvs<FlashStorage<'d>>,
    hasher: Hasher<'d>,
    config_updated: bool,
}

#[derive(Debug, defmt::Format)]
pub enum StorageError {
    Flash(esp_storage::FlashStorageError),
    Partition(partitions::Error),
    PartitionNotFound,
    Nvs(NvsError),
}

impl From<FlashStorageError> for StorageError {
    fn from(e: FlashStorageError) -> Self {
        StorageError::Flash(e)
    }
}

impl From<partitions::Error> for StorageError {
    fn from(e: partitions::Error) -> Self {
        StorageError::Partition(e)
    }
}

impl From<NvsError> for StorageError {
    fn from(e: NvsError) -> Self {
        StorageError::Nvs(e)
    }
}

impl<'d> Storage<'d> {
    fn wasm_key_from_name(&mut self, name: &str) -> Key {
        // create ascii only hash for widget name
        let digest = self.hasher.hash(name);
        let mut key_bytes = [b'0'; 15];
        const HEX: &[u8; 16] = b"0123456789abcdef";

        for i in 0..7 {
            key_bytes[2 * i] = HEX[(digest[i] >> 4) as usize];
            key_bytes[2 * i + 1] = HEX[(digest[i] & 0x0f) as usize];
        }
        key_bytes[14] = HEX[(digest[7] >> 4) as usize];

        Key::from_array(&key_bytes)
    }

    pub fn new(flash: FLASH<'d>, sha_peripherals: SHA<'d>) -> Result<Self, StorageError> {
        let mut flash_storage = FlashStorage::new(flash).multicore_auto_park();

        // read partition table using esp_bootloader_esp_idf
        // heap-allocated (→ PSRAM) to avoid large stack frame during init
        let mut partition_table_buffer =
            alloc::boxed::Box::new([0u8; partitions::PARTITION_TABLE_MAX_LEN]);
        let partition_table =
            partitions::read_partition_table(&mut flash_storage, &mut *partition_table_buffer)?;

        // list partitions
        defmt::info!("Partition table:");
        for partition in partition_table.iter() {
            defmt::info!(
                "  {}: offset=0x{:x}, size=0x{:x}",
                partition.label_as_str(),
                partition.offset(),
                partition.len()
            );
        }

        // find the combined storage partition
        let storage = partition_table
            .iter()
            .find(|p| p.label_as_str() == "storage")
            .ok_or(StorageError::PartitionNotFound)?;

        let nvs = Nvs::new(
            storage.offset() as usize,
            storage.len() as usize,
            flash_storage,
        )?;

        Ok(Self {
            nvs,
            hasher: Hasher::new(sha_peripherals),
            config_updated: false,
        })
    }

    pub fn save_system_config(
        &mut self,
        system_config: &SystemConfiguration,
    ) -> Result<(), StorageError> {
        // only save if config changed to avoid flash wear
        if let Ok(current_config) = self.get_system_config()
            && current_config == *system_config
        {
            info!("System config unchanged, not saving to flash");
            return Ok(());
        }

        let value = serde_json::to_string(system_config)
            .map_err(|_| StorageError::Nvs(NvsError::FlashError))?;
        self.config_set("system_config", &value)?;
        self.config_updated = true;
        Ok(())
    }

    pub fn get_system_config(&mut self) -> Result<SystemConfiguration, StorageError> {
        let value: alloc::string::String = self.config_get("system_config")?;
        let config: SystemConfiguration =
            serde_json::from_str(&value).map_err(|_| StorageError::Nvs(NvsError::FlashError))?;
        Ok(config)
    }

    pub fn get_system_config_change(&mut self) -> Option<SystemConfiguration> {
        if self.config_updated {
            self.config_updated = false;
            match self.get_system_config() {
                Ok(config) => Some(config),
                Err(err) => {
                    info!("Error getting updated config: {:?}", err);
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn save_compiled_widget(
        &mut self,
        name: &str,
        description: &str,
        version: &str,
        json_config: &str,
        update_cycle_seconds: u32,
        data: &[u8],
    ) -> Result<(), StorageError> {
        self.wasm_write(name, data)?;
        let mut config = self.get_system_config()?;
        config.widgets.push(WidgetInstallationData {
            name: name.to_string(),
            description: description.to_string(),
            version: version.to_string(),
            json_config: json_config.to_string(),
            update_cycle_seconds,
        });
        self.save_system_config(&config)?;
        Ok(())
    }

    pub fn deinstall_widget(&mut self, name: &str) -> Result<(), StorageError> {
        // self.wasm_read(name)?; // check if widget exists
        self.wasm_delete(name)?; // remove widget data
        let mut config = self.get_system_config()?;
        config.widgets.retain(|w| w.name != name);
        self.save_system_config(&config)?;
        Ok(())
    }

    pub fn config_set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        info!("Setting config for key '{}'", key);
        let ns = Key::from_str("config");
        let k = Key::from_str(key);
        self.nvs.set(&ns, &k, value)?;
        Ok(())
    }

    pub fn config_get(&mut self, key: &str) -> Result<alloc::string::String, StorageError> {
        info!("Getting config for key '{}'", key);
        let ns = Key::from_str("config");
        let k = Key::from_str(key);
        Ok(self.nvs.get(&ns, &k)?)
    }

    pub fn wasm_write(&mut self, name: &str, data: &[u8]) -> Result<(), StorageError> {
        let key = self.wasm_key_from_name(name);
        let ns = Key::from_str("wasm");
        info!(
            "Writing WASM binary with name: '{}' and key: {:?}",
            name, key
        );
        self.nvs.set(&ns, &key, data)?;
        Ok(())
    }

    pub fn wasm_read(&mut self, name: &str) -> Result<alloc::vec::Vec<u8>, StorageError> {
        let key = self.wasm_key_from_name(name);
        let ns = Key::from_str("wasm");
        info!(
            "Reading WASM binary with name: '{}' and key: {:?}",
            name, key
        );
        Ok(self.nvs.get(&ns, &key)?)
    }

    pub fn wasm_delete(&mut self, name: &str) -> Result<(), StorageError> {
        let key = self.wasm_key_from_name(name);
        let ns = Key::from_str("wasm");
        info!(
            "Deleting WASM binary with name: '{}' and key: {:?}",
            name, key
        );
        self.nvs.delete(&ns, &key)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn list_widgets(&mut self) -> Result<alloc::vec::Vec<alloc::string::String>, StorageError> {
        let config = self.get_system_config()?;
        Ok(config.widgets.iter().map(|w| w.name.clone()).collect())
    }
}
