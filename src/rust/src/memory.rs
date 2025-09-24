use soroban_env_host::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host::xdr::{LedgerEntry, LedgerKey};
use soroban_env_host::HostError;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Default, Clone)]
pub struct Memory {
    memory: RefCell<BTreeMap<Rc<LedgerKey>, (Rc<LedgerEntry>, Option<u32>)>>,
}

impl Memory {
    pub fn insert(&self, entry: LedgerEntry) {
        self.memory
            .borrow_mut()
            .insert(Rc::new(entry.to_key()), (Rc::new(entry), None));
    }

    pub fn remove(&self, key: &Rc<LedgerKey>) {
        let entry = self.memory.borrow_mut().remove(key);
    }
}

impl SnapshotSource for Memory {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
        let entry = self.memory.borrow().get(key).cloned();

        Ok(entry)
    }
}
