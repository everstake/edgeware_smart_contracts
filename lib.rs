#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract]
mod edgeware_bridge {
    use erc20token::ERC20Token;

    use ink_prelude::vec::Vec;
    use ink_prelude::string::String;

    use ink_storage::{collections::{HashMap as StorageHashMap},
                    traits::{
                        PackedLayout,
                        SpreadLayout,
                    }};
    use scale::{Decode, Encode};
    use sha3::{Digest, Sha3_256};

    const ZERO_ADDRESS_BYTES: [u8; 32] = [0; 32];
    const ONE_DAY: u64 = 86400;

    #[ink(storage)]
    pub struct EdgewareBridge {
        swap_requests: StorageHashMap<Vec<u8>, Vec<AccountId>>,
        tokens: StorageHashMap<AccountId, bool>,
        validators: StorageHashMap<AccountId, bool>,
        daily_limit: StorageHashMap<AccountId, u128>,
        daily_spend: StorageHashMap<AccountId, u128>,
        daily_limit_set_time: StorageHashMap<AccountId, u64>,
        validator_rewards: StorageHashMap<AccountId, u128>,
        fee: u128,
        signature_threshold: u16,
        max_validator_count: u16,
        tx_expiration_time: u64,
        owner: AccountId,
        transfer_nonce: u128,
        token_contract: ERC20Token,
        chain_id: u8,
        min_amount_to_transfer: u128,
    }

    /// Emitted when an user want to make cross chain transfer
    #[ink(event)]
    pub struct Transfer {
        receiver: String,
        sender: AccountId,
        amount: u128,
        asset: AccountId,
        transfer_nonce: u128,
        timestamp: u64,
    }

    #[derive(Encode, Decode, SpreadLayout, PackedLayout, Clone)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink_storage::traits::StorageLayout
        )
    )]
    pub struct SwapMessage {
        pub chain_id: u8,
        pub receiver: AccountId,
        pub sender: String,
        pub timestamp: u64,
        pub amount: u128,
        pub asset: AccountId,
        pub transfer_nonce: u128,
    }

    impl EdgewareBridge {
        #[ink(constructor)]
        pub fn new(
            threshold: u16,
            max_permissible_validator_count: u16,
            transfer_fee: u128,
            coin_daily_limit: u128,
            token_contract: ERC20Token,
            chain_id: u8,
            min_amount_to_transfer: u128
        ) -> Self {
            let caller = Self::env().caller();
            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);
            let mut daily_limit: StorageHashMap<AccountId, u128> = StorageHashMap::default();
            let current_timestamp: u64 = Self::env().block_timestamp() / 1000;
            daily_limit.insert(zero_address, coin_daily_limit);
            let mut daily_limit_set_time: StorageHashMap<AccountId, u64> = StorageHashMap::default();
            daily_limit_set_time.insert(zero_address, current_timestamp);
            let mut daily_spend: StorageHashMap<AccountId, u128> = StorageHashMap::default();
            daily_spend.insert(zero_address, 0);
            Self {
                swap_requests: StorageHashMap::default(),
                tokens: StorageHashMap::default(),
                validators: StorageHashMap::default(),
                daily_spend,
                fee: transfer_fee,
                signature_threshold: threshold,
                max_validator_count: max_permissible_validator_count,
                tx_expiration_time: ONE_DAY,
                owner: caller,
                transfer_nonce: 0,
                daily_limit,
                daily_limit_set_time,
                validator_rewards: StorageHashMap::default(),
                token_contract,
                chain_id,
                min_amount_to_transfer,
            }
        }

        #[ink(message)]
        pub fn transfer_ownership(&mut self, new_owner: AccountId) {
            self.ensure_owner(self.env().caller());
            self.owner = new_owner;
        }

        #[ink(message)]
        pub fn set_fee(&mut self, new_fee: u128) {
            self.ensure_owner(self.env().caller());
            assert!(new_fee > 0 || new_fee < 100, "Fee should be between 0 and 100");
            self.fee = new_fee;
        }

        #[ink(message)]
        pub fn add_validator(&mut self, new_validator: AccountId) {
            self.ensure_owner(self.env().caller());
            let count_acrive_validators: u16 = self.validators.len() as u16;
            assert!(count_acrive_validators + 1 <= self.max_validator_count, "Count of Validators already reach maximum");
            self.validators.insert(new_validator, true);
        }

        #[ink(message)]
        pub fn remove_validator(&mut self, validator: AccountId) {
            self.ensure_owner(self.env().caller());
            assert!((self.validators.len() - 1) as u16 >= self.signature_threshold, "Count of Validators can't be less than necessary threshold of approvals");
            assert_eq!(self.validators.take(&validator).is_some(), true);
        }

        #[ink(message)]
        pub fn set_threshold(&mut self, new_signature_threshold: u16) {
            self.ensure_owner(self.env().caller());
            assert!(new_signature_threshold > 0 && new_signature_threshold <= self.max_validator_count, "Signature threshold must be more than zero and less or equal maximum validators count");
            self.signature_threshold = new_signature_threshold;
        }

        #[ink(message)]
        pub fn add_token(&mut self, new_token: AccountId, token_daily_limit: u128) {
            self.ensure_owner(self.env().caller());
            self.tokens.insert(new_token, true);
            assert!(token_daily_limit > 0, "Token daily limit must be more than zero");
            self.daily_limit.insert(new_token, token_daily_limit);
            self.daily_limit_set_time.insert(new_token, self.env().block_timestamp() / 1000);
            self.daily_spend.insert(new_token, 0);
        }

        #[ink(message)]
        pub fn remove_token(&mut self, token: AccountId) {
            self.ensure_owner(self.env().caller());
            assert_eq!(self.tokens.take(&token).is_some(), true);
            assert_eq!(self.daily_limit.take(&token).is_some(), true);
            assert_eq!(self.daily_limit_set_time.take(&token).is_some(), true);
        }

        #[ink(message)]
        pub fn set_daily_limit(&mut self, new_limit: u128, asset_limited: AccountId) {
            self.ensure_owner(self.env().caller());
            assert_eq!(self.tokens.contains_key(&asset_limited), true);
            assert!(new_limit > 0, "Daily limit must be more than zero");
            self.daily_limit.insert(asset_limited, new_limit);
        }

        #[ink(message)]
        pub fn set_tx_expiration_time(&mut self, new_tx_expiration_time: u64) {
            self.ensure_owner(self.env().caller());
            assert!(new_tx_expiration_time > 0, "Transaction expiration time must be greater than zero");
            self.tx_expiration_time = new_tx_expiration_time;
        }

        #[ink(message)]
        pub fn is_token_in(&self, token: AccountId) -> bool {
            self.tokens.contains_key(&token)
        }

        #[ink(message)]
        pub fn is_validator_in(&self, validator: AccountId) -> bool {
            self.validators.contains_key(&validator)
        }

        #[ink(message)]
        pub fn is_swap_request_in(&self, swap_hash: Vec<u8>) -> bool {
            self.swap_requests.contains_key(&swap_hash)
        }

        #[ink(message)]
        pub fn get_daily_limit(&self, token: AccountId) -> u128 {
            self.daily_limit.get(&token).unwrap().clone()
        }

        #[ink(message)]
        pub fn get_daily_spend(&self, token: AccountId) -> u128 {
            self.daily_spend.get(&token).unwrap().clone()
        }

        #[ink(message)]
        pub fn get_daily_limit_set_time(&self, token: AccountId) -> u64 {
            self.daily_limit_set_time.get(&token).unwrap().clone()
        }

        #[ink(message)]
        pub fn get_fee(&self) -> u128 {
            self.fee
        }

        #[ink(message)]
        pub fn get_signature_threshold(&self) -> u16 {
            self.signature_threshold
        }

        #[ink(message)]
        pub fn get_max_validator_count(&self) -> u16 {
            self.max_validator_count
        }

        #[ink(message)]
        pub fn get_tx_expiration_time(&self) -> u64 {
            self.tx_expiration_time
        }

        #[ink(message)]
        pub fn get_transfer_nonce(&self) -> u128 {
            self.transfer_nonce
        }

        #[ink(message)]
        pub fn get_tokens(&self) -> Vec<AccountId> {
            let mut tokens: Vec<AccountId> = Vec::new();
            for el in self.tokens.keys() {
                tokens.push(el.clone());
            }
            tokens
        }

        #[ink(message)]
        pub fn get_validators(&self) -> Vec<AccountId> {
            let mut validators: Vec<AccountId> = Vec::new();
            for el in self.validators.keys() {
                validators.push(el.clone());
            }
            validators
        }

        #[ink(message)]
        pub fn get_chain_id(&self) -> u8 {
            self.chain_id
        }

        #[ink(message)]
        pub fn get_request_swaps(&self) -> Vec<Vec<u8>> {
            let mut swaps: Vec<Vec<u8>> = Vec::new();
            for el in self.swap_requests.keys() {
                swaps.push(el.clone());
            }
            swaps
        }

        #[ink(message)]
        pub fn get_count_of_approvals(&self, message_hash: Vec<u8>) -> u16 {
            let validators_who_approved_swap: Option<Vec<AccountId>> = self.get_validators_who_approved(&message_hash);
            match validators_who_approved_swap {
                Some(n) => {
                    return n.len() as u16;
                },
                None => {
                    return 0;
                }
            }
        }

        #[ink(message)]
        pub fn get_validator_rewards(&self, validator: AccountId) -> u128 {
            let rewards_amount: Option<&u128> = self.validator_rewards.get(&validator);

            match rewards_amount {
                Some(rew) => return *rew,
                None => return 0,
            }
        }

        //  This method necessary such as there can be not relevant and did not approved requests which will take storage
        //  and to clean the storage and save some money owner can use this method
        #[ink(message)]
        pub fn clean_request_swaps_list(&mut self) {
            self.ensure_owner(self.env().caller());
            self.swap_requests = StorageHashMap::default();
        }

        #[ink(message)]
        pub fn request_rewards(&mut self) {
            let caller: AccountId = self.env().caller();
            let rewards_amount: Option<&u128> = self.validator_rewards.get(&caller);
            assert!(rewards_amount.is_some(), "This address doesn't have any rewards");

            assert!(self.env().transfer(caller, *rewards_amount.unwrap()).is_ok(), "Error while transfer rewards to the validator");

            self.validator_rewards.take(&caller);
        }

        // Validator method
        #[ink(message)]
        pub fn request_swap(&mut self, transfer_info: SwapMessage) {
            let caller: AccountId = self.env().caller();
            assert!(self.validators.get(&caller).is_some(), "Only Validator can send requests to swap assets");

            assert!(transfer_info.chain_id == self.chain_id, "Swap request's chain ID doesn't match contract's chain ID");

            assert!(self.check_expiration_time(transfer_info.timestamp.clone()), "Transaction can't be sent because of expiration time");

            assert!(self.check_asset(&transfer_info.asset), "Unknown asset is trying to transfer");

            let message_hash: Vec<u8> = self.hash_message(transfer_info.clone());

            let validators_who_approved_swap: Option<Vec<AccountId>> = self.get_validators_who_approved(&message_hash);
            match validators_who_approved_swap {
                Some(n) => {
                    assert!(self.is_in(&n, &caller) == false, "This Validator has already sent approval");
                    if (n.len() as u16) + 1 >= self.signature_threshold {
                        self.make_swap(transfer_info.asset, transfer_info.amount, transfer_info.receiver);
                        self.swap_requests.take(&message_hash);
                    } else {
                        let mut updated_validator_list: Vec<AccountId> = n.clone();
                        updated_validator_list.push(caller);
                        self.swap_requests.insert(message_hash, updated_validator_list);
                    }
                },
                None => {
                    let mut init_vec_of_validators: Vec<AccountId> = Vec::new();
                    init_vec_of_validators.push(caller);
                    self.swap_requests.insert(message_hash, init_vec_of_validators);
                }
            }
        }

        // User method
        #[ink(message, payable)]
        pub fn transfer_coin(&mut self, receiver: String) -> bool {
            let attached_deposit: u128 = self.env().transferred_balance();
            
            assert!(attached_deposit >= self.min_amount_to_transfer, "Transfer amount should be bigger or equal than minimum required amount");

            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);

            self.check_asset_daily_limit(&zero_address, attached_deposit);

            self.increase_transfer_nonce();

            self.distribute_rewards_for_validators(attached_deposit);

            self.env().emit_event(Transfer {
                receiver,
                sender: self.env().caller(),
                amount: attached_deposit,
                asset: zero_address,
                transfer_nonce: self.transfer_nonce,
                timestamp: self.env().block_timestamp() / 1000,
            });
            true
        }

        // User method
        #[ink(message)]
        pub fn transfer_token(&mut self, receiver: String, amount: u128, asset: AccountId) -> bool {
            assert!(amount >= self.min_amount_to_transfer, "Transfer amount should be bigger or equal than minimum required amount");
            assert!(self.check_asset(&asset), "Unknown asset is trying to transfer");
            self.check_asset_daily_limit(&asset, amount);
            let caller: AccountId = self.env().caller();
            assert!(self.token_contract.balance_of(caller) >= amount, "Sender doesn't have enough tokens to make transfer");
            assert!(self.token_contract.burn(amount.clone(), caller), "Error while burn sender's tokens");
            self.increase_transfer_nonce();

            self.env().emit_event(Transfer {
                receiver,
                sender: self.env().caller(),
                amount,
                asset,
                transfer_nonce: self.transfer_nonce,
                timestamp: self.env().block_timestamp() / 1000,
            });
            true
        }

        fn distribute_rewards_for_validators(&mut self, amount: u128) {
            let rewards_amount: u128 = (amount * self.fee) / 100;

            for (validator_address, is_validator) in self.validators.iter() {
                if *is_validator {
                    let existing_rewards = self.validator_rewards.get(validator_address);
                    match existing_rewards {
                        Some(rew) => {
                            let updated_reward: u128 = rew + (rewards_amount / self.validators.len() as u128);
                            self.validator_rewards.insert(validator_address.clone(), updated_reward);
                        },
                        None => {
                            let reward: u128 = rewards_amount / self.validators.len() as u128;
                            self.validator_rewards.insert(validator_address.clone(), reward);
                        }
                    }
                }
            }
        }

        fn check_asset_daily_limit(&mut self, asset: &AccountId, amount: u128) {
            let asset_daily_limit: u128 = self.daily_limit.get(asset).unwrap().clone();

            assert!(asset_daily_limit > 0, "Can't transfer asset without daily limit");

            self.update_daily_limit(asset);

            let asset_daily_spent: u128 = self.daily_spend.get(asset).unwrap().clone();

            assert!(amount + asset_daily_spent <= asset_daily_limit);

            self.daily_spend.insert(*asset, asset_daily_spent + amount);
        }

        fn get_validators_who_approved(&self, message_hash: &Vec<u8>) -> Option<Vec<AccountId>> {
            let validators: Option<&Vec<AccountId>> = self.swap_requests.get(message_hash);
            match validators {
                Some(n) => return Some(n.clone()),
                None => return None,
            }
        }

        fn is_in(&self, list_of_accs: &Vec<AccountId>, new_acc: &AccountId) -> bool {
            for acc in list_of_accs.iter() {
                if acc == new_acc {
                    return true;
                }
            }
            return false;
        }

        fn increase_transfer_nonce(&mut self) {
            self.transfer_nonce = self.transfer_nonce + 1;
        }

        fn check_asset(&self, asset_address: &AccountId) -> bool {
            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);
            if asset_address == &zero_address || self.tokens.contains_key(asset_address) {
                return true;
            } else {
                return false;
            }
        }

        fn hash_message(&self, swap_message: SwapMessage) -> Vec<u8> {
            let encoded: Vec<u8> = swap_message.encode();
            let mut hasher = Sha3_256::new();
            hasher.input(encoded.as_slice());
            let result = hasher.result();
            result.to_vec()
        }



        fn check_expiration_time(&self, tx_time: u64) -> bool {
            let current_time: u64 = self.env().block_timestamp() / 1000;
            
            if current_time - tx_time > self.tx_expiration_time {
                return false;
            } else {
                return true;
            }
        }

        fn update_daily_limit(&mut self, asset: &AccountId) {
            let current_time: u64 = self.env().block_timestamp() / 1000;

            let last_limit_set_time: &u64 = self.daily_limit_set_time.get(asset).unwrap();
            if current_time - last_limit_set_time > ONE_DAY {
                self.daily_limit_set_time.insert(asset.clone(), current_time);
                self.daily_spend.insert(asset.clone(), 0);
            }
        }

        fn make_swap(&mut self, asset: AccountId, amount: u128, receiver: AccountId) {
            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);
            
            if asset == zero_address {
                let amount_to_send: u128 = amount - (amount * self.fee / 100);
                self.distribute_rewards_for_validators(amount);
                assert!(self.env().transfer(receiver, amount_to_send).is_ok(), "Error while transfer coins to the receiver");
            } else {
                let amount_to_send: u128 = amount - (amount * self.fee / 100);
                assert!(self.token_contract.mint(amount_to_send, receiver), "Error while mint tokens for the receiver");
            }
        }

        fn ensure_owner(&self, caller: AccountId) {
            assert_eq!(caller, self.owner, "This method can be called only by owner");
        }
    }

    #[cfg(test)]
    mod tests {
    }
}
