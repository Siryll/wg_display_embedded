#![allow(dead_code)]
use common::models::{SystemConfiguration, WidgetInstallationData};
use alloc::string::ToString;
use defmt::info;
use esp_bootloader_esp_idf::partitions;
use esp_hal::peripherals::FLASH;
use esp_nvs::{Key, Nvs, error::Error as NvsError};
use esp_storage::{FlashStorage, FlashStorageError};

pub struct Storage<'d> {
    nvs: Nvs<FlashStorage<'d>>,
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
    pub fn new(flash: FLASH<'d>) -> Result<Self, StorageError> {
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

        Ok(Self { nvs })
    }

    pub fn save_widget_config(
        &mut self,
        system_config: &SystemConfiguration,
    ) -> Result<(), StorageError> {
        let ns = Key::from_str("config");
        let k = Key::from_str("system_config");
        let value = serde_json::to_string(system_config)
            .map_err(|_| StorageError::Nvs(NvsError::FlashError))?;
        self.nvs.set(&ns, &k, value.as_str())?;
        Ok(())
    }

    pub fn get_widget_config(&mut self) -> Result<SystemConfiguration, StorageError> {
        let ns = Key::from_str("config");
        let k = Key::from_str("system_config");
        let value: alloc::string::String = self.nvs.get(&ns, &k)?;
        let config: SystemConfiguration =
            serde_json::from_str(&value).map_err(|_| StorageError::Nvs(NvsError::FlashError))?;
        Ok(config)
    }

    pub fn save_compiled_widget(
        &mut self,
        name: &str,
        description: &str,
        version: &str,
        json_config: &str,
        data: &[u8],
    ) -> Result<(), StorageError> {
        self.wasm_write(name, data)?;
        let mut config = self.get_widget_config()?;
        config.widgets.push(WidgetInstallationData {
            name: name.to_string(),
            description: description.to_string(),
            version: version.to_string(),
            json_config: json_config.to_string(),
        });
        self.save_widget_config(&config)?;
        Ok(())
    }

    pub fn deinstall_widget(&mut self, name: &str) -> Result<(), StorageError> {
        self.wasm_read(name)?; // check if widget exists
        self.wasm_delete(name)?; // remove widget data
        let mut config = self.get_widget_config()?;
        config.widgets.retain(|w| w.name != name);
        self.save_widget_config(&config)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn config_set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        info!("Setting config for key '{}'", key);
        let ns = Key::from_str("config");
        let k = Key::from_str(key);
        self.nvs.set(&ns, &k, value)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn config_get(&mut self, key: &str) -> Result<alloc::string::String, StorageError> {
        info!("Getting config for key '{}'", key);
        let ns = Key::from_str("config");
        let k = Key::from_str(key);
        Ok(self.nvs.get(&ns, &k)?)
    }

    #[allow(dead_code)]
    pub fn wasm_write(&mut self, name: &str, data: &[u8]) -> Result<(), StorageError> {
        info!("Writing WASM binary with name: '{}'", name);
        let ns = Key::from_str("wasm");
        let k = Key::from_str(name);
        self.nvs.set(&ns, &k, data)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn wasm_read(&mut self, name: &str) -> Result<alloc::vec::Vec<u8>, StorageError> {
        info!("Reading WASM binary with name: '{}'", name);
        let ns = Key::from_str("wasm");
        let k = Key::from_str(name);
        Ok(self.nvs.get(&ns, &k)?)
    }

    #[allow(dead_code)]
    pub fn wasm_delete(&mut self, name: &str) -> Result<(), StorageError> {
        info!("Deleting WASM binary with name: '{}'", name);
        let ns = Key::from_str("wasm");
        let k = Key::from_str(name);
        self.nvs.delete(&ns, &k)?;
        Ok(())
    }

    pub fn list_widgets(&mut self) -> Result<alloc::vec::Vec<alloc::string::String>, StorageError> {
        let config = self.get_widget_config()?;
        Ok(config.widgets.iter().map(|w| w.name.clone()).collect())
    }
}
