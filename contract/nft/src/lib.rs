/*!
Non-Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
*/
use near_contract_standards::non_fungible_token::core::{
    NonFungibleTokenCore, NonFungibleTokenResolver
};

//use near_contract_standards::non_fungible_token::approval::ext_nft_approval_receiver;


use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata, NFT_METADATA_SPEC,
};
use near_contract_standards::non_fungible_token::{TokenId};
use near_contract_standards::non_fungible_token::NonFungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{
    env, near_bindgen, AccountId, BorshStorageKey, PanicOnDefault, Promise,
    Balance, serde_json::json, assert_one_yocto, Gas, ext_contract, PromiseOrValue,
};

use near_sdk::collections::{LazyOption, UnorderedMap, UnorderedSet};

/* custon codigo */
use near_sdk::json_types::{/*ValidAccountId,*/ U128, U64};

use serde::Serialize;
use serde::Deserialize;
use std::collections::HashMap;
use near_sdk::env::is_valid_account_id;
pub mod event;
pub use event::NearEvent;


pub const TOKEN_DELIMETER: char = ':';
pub const TITLE_DELIMETER: &str = " #";
pub const VAULT_FEE: u128 = 500;


const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(10_000_000_000_000);
const GAS_FOR_NFT_TRANSFER_CALL: Gas = Gas(40_000_000_000_000); //GAS_FOR_NFT_TRANSFER_CALL(30_000_000_000_000) + GAS_FOR_RESOLVE_TRANSFER;
//const GAS_FOR_NFT_APPROVE: Gas = Gas(10_000_000_000_000);
//const GAS_FOR_MINT: Gas = Gas(90_000_000_000_000);
//const NO_DEPOSIT: Balance = 0;
//const MAX_PRICE: Balance = 1_000_000_000 * 10u128.pow(24);
const CURRENT_TRANSACTION_FEE: Balance = 200;

pub type TokenSeriesId = String;

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Payout {
    pub payout: HashMap<AccountId, U128>,
}


/* codigo customizado */

#[ext_contract(ext_non_fungible_token_receiver)]
trait NonFungibleTokenReceiver {
    /// Returns `true` if the token should be returned back to the sender.
    fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: TokenId,
        msg: String,
    ) -> Promise;
}

#[ext_contract(ext_self)]
trait NonFungibleTokenResolverExt {
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<HashMap<AccountId, u64>>,
    ) -> bool;
}



#[derive(BorshDeserialize, BorshSerialize)]
pub struct TokenSeries {
	metadata: TokenMetadata,
	creator_id: AccountId,
	tokens: UnorderedSet<TokenId>,
    price: Option<f64>,
    is_mintable: bool,
    royalty: HashMap<AccountId, u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct RoyaltyBuy {
	artist_id: String,
	porcentaje: String,
    amount: String,
    amount_usd: String,
    tax: String
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenSeriesJson {
    token_series_id: TokenSeriesId,
	metadata: TokenMetadata,
	creator_id: AccountId,
    royalty: HashMap<AccountId, u32>
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenSeriesJson2 {
	token_series_id: TokenSeriesId,
    metadata: TokenMetadata,
	creator_id: AccountId,
    price: Option<Balance>,
    price_usd: Option<f64>,
    is_mintable: bool,
    royalty: HashMap<AccountId, u32>
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenSeriesJson3 {
    token_series_id: TokenSeriesId,
	metadata: TokenMetadata,
	creator_id: AccountId,
    royalty: HashMap<AccountId, u32>,
    transaction_fee: U128
}


#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokensView {
	owner_id: String,
    token_id: String
}



#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenCustom {
    token_id: TokenId,
    owner_id: AccountId,
    metadata: Option<TokenMetadata>,
    approved_account_ids: Option<HashMap<AccountId, u64>>,
    royalty: Option<HashMap<AccountId, u32>>
}

/* fin codigo costumizado */

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    tokens: NonFungibleToken,
    metadata: LazyOption<NFTContractMetadata>,
    /* codigo costumizado */
    owner_id: AccountId,
    list_admin: UnorderedSet<AccountId>,
    id_serie: u128,
    token_series_by_id: UnorderedMap<TokenSeriesId, TokenSeries>,
    vault_id: AccountId,
    tasa: f64,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    NonFungibleToken,
    Metadata,
    TokenMetadata,
    Enumeration,
    Approval,
    /*codigo costumizado*/
    AdminKey,
    TokenSeriesById,
    TokensBySeriesInner { token_series: String },
    TokensPerOwner { account_hash: Vec<u8> },
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract owned by `owner_id` with
    /// default metadata (for example purposes only).
    #[init]
    pub fn new_default_meta(owner_id: AccountId, vault_id: AccountId,) -> Self {
        Self::new(
            owner_id,
            vault_id,
            NFTContractMetadata {
                spec: NFT_METADATA_SPEC.to_string(),
                name: "Meta democracia".to_string(),
                symbol: "MetaDemocracia".to_string(),
                icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                base_uri: None,
                reference: None,
                reference_hash: None,
            },
        )
    }

    #[init]
    pub fn new(owner_id: AccountId, vault_id: AccountId, metadata: NFTContractMetadata) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        metadata.assert_valid();
        Self {
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                owner_id.clone(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            /* codigo costumizado */
            owner_id: owner_id,
            list_admin: UnorderedSet::new(StorageKey::AdminKey),
            id_serie: 1,
            token_series_by_id: UnorderedMap::new(StorageKey::TokenSeriesById),
            vault_id: vault_id,
            tasa: 0.0,
        }
    }

    /* codigo original */
    /*
    /// Mint a new token with ID=`token_id` belonging to `receiver_id`.
    ///
    /// Since this example implements metadata, it also requires per-token metadata to be provided
    /// in this call. `self.tokens.mint` will also require it to be Some, since
    /// `StorageKey::TokenMetadata` was provided at initialization.
    ///
    /// `self.tokens.mint` will enforce `predecessor_account_id` to equal the `owner_id` given in
    /// initialization call to `new`.
    #[payable]
    pub fn nft_mint(
        &mut self,
        token_id: TokenId,
        receiver_id: AccountId,
        token_metadata: TokenMetadata,
    ) -> Token {
        self.tokens.mint(token_id, receiver_id, Some(token_metadata))
    }*/

    /* codigo custom */
    pub fn edit_vault(&mut self, account_id: AccountId){
        assert!(self.owner_id == env::signer_account_id() || self.list_admin.contains(&env::signer_account_id()), "Only administrator");
        self.vault_id = account_id;
    }

    // cargar usuarios a la lista de administradores
    // solo los administradores pueden usar esta funcion
    pub fn add_admin(&mut self, account_id: AccountId) {
        assert!(self.owner_id == env::signer_account_id() || self.list_admin.contains(&env::signer_account_id()), "Only administrator");
        self.list_admin.insert(&account_id.clone());

        env::log_str(
            &json!({
                "type": "add_admin",
                "params": {
                    "account_id": account_id.to_string()
                }
            })
            .to_string(),
        );

    }


    pub fn update_tasa(&mut self, tasa: f64) -> f64 {
        assert!(self.owner_id == env::signer_account_id() || self.list_admin.contains(&env::signer_account_id()), "Only administrator");        
        assert!(tasa > 0.0, "La tasa debe ser mayor a 0");
        
        self.tasa = tasa;

        env::log_str(
            &json!({
                "type": "update_tasa",
                "params": {
                    "tasa": self.tasa
                }
            })
            .to_string(),
        );
        self.tasa
    }

    pub fn get_tasa(self) -> f64 {
        self.tasa
    }

   #[payable]
    pub fn update_nft_series(&mut self, 
        token_series_id: TokenSeriesId, 
        title: Option<String>,
        description: Option<String>,
        media: Option<String>,
        price: Option<f64>,
        extra: Option<String>,
        royalty: Option<HashMap<AccountId, u32>>,
    ) -> TokenSeriesJson {
        assert!(self.owner_id == env::signer_account_id() || self.list_admin.contains(&env::signer_account_id()), "Only administrator");

        //let initial_storage_usage = env::storage_usage();
        

        let mut nft_serie = self.token_series_by_id.get(&token_series_id).expect("tonken serie id not exist");
        if title.is_some() { nft_serie.metadata.title = title; }
        if description.is_some() { nft_serie.metadata.description = description; }
        if media.is_some() { nft_serie.metadata.media = media; }
        if price.is_some() {
            assert_eq!(
                nft_serie.is_mintable,
                true,
                "Token series is not mintable"
            );
            
            if price.unwrap() > 0.0 {
                let porcentaje_adicional: f64 = 1.05;
                let price_final: f64 = price.unwrap() * porcentaje_adicional; 
                nft_serie.price = Some(price_final);
            } else {
                nft_serie.price = None;
            }
        }

        if extra.is_some() { nft_serie.metadata.extra = extra; }
        
        if royalty.is_some() {
            let mut total_perpetual = 0;
            let mut total_accounts = 0;
            let royalty_res: HashMap<AccountId, u32> = if let Some(royalty) = royalty {
                for (k , v) in royalty.iter() {
                    if !is_valid_account_id(k.as_bytes()) {
                        env::panic_str("Not valid account_id for royalty");
                    };
                    total_perpetual += *v;
                    total_accounts += 1;
                }
                royalty
            } else {
                HashMap::new()
            };

            assert!(total_accounts <= 10, "royalty exceeds 10 accounts");

            assert!(
                total_perpetual <= 9000,
                "Exceeds maximum royalty -> 9000",
            );
            nft_serie.royalty = royalty_res;
        }

        self.token_series_by_id.insert(&token_series_id, &nft_serie);

        let precio: Option<String> = if nft_serie.price.is_some() {
            Some(nft_serie.price.unwrap().to_string())
        } else {
            None
        };

        env::log_str(
            &json!({
                "type": "update_nft_series",
                "params": {
                    "token_series_id": token_series_id,
                    "token_metadata": nft_serie.metadata.clone(),
                    "creator_id": nft_serie.creator_id.clone(),
                    "price": precio,
                    "royalty": nft_serie.royalty.clone(),
                }
            })
            .to_string(),
        );

        //refund_deposit(env::storage_usage() - initial_storage_usage, 0);


        TokenSeriesJson {
            token_series_id,
			metadata: nft_serie.metadata.clone(),
			creator_id: nft_serie.creator_id.clone(),
            royalty: nft_serie.royalty,
		}
    }
 

    #[payable]
    pub fn nft_series(
        &mut self,
        token_metadata: TokenMetadata,
        price: Option<f64>,
        royalty: Option<HashMap<AccountId, u32>>,
    ) -> TokenSeriesJson {
        assert!(self.owner_id == env::signer_account_id() || self.list_admin.contains(&env::signer_account_id()), "Only administrator");
        assert!((self.tasa > 0.0), "Tasa debe ser mayor a 0");
        

        let initial_storage_usage = env::storage_usage();
        let caller_id = env::signer_account_id();

        let token_series_id: String = self.id_serie.to_string();
        

        assert!(
            self.token_series_by_id.get(&token_series_id).is_none(),
            "duplicate token_series_id"
        );

        let title = token_metadata.title.clone();
        assert!(title.is_some(), "token_metadata.title is required");
        
        
        let mut total_perpetual = 0;
        let mut total_accounts = 0;

        let royalty_res: HashMap<AccountId, u32> = if let Some(royalty) = royalty {
            for (k , v) in royalty.iter() {
                if !is_valid_account_id(k.as_bytes()) {
                    env::panic_str("Not valid account_id for royalty");
                };
                total_perpetual += *v;
                total_accounts += 1;
            }
            royalty
        } else {
            HashMap::new()
        };

        assert!(total_accounts <= 10, "royalty exceeds 10 accounts");

        assert!(
            total_perpetual <= 9000,
            "Exceeds maximum royalty -> 9000",
        );

        let price_res: Option<f64> = if price.is_some() {
            let porcentaje_adicional: f64 = 1.05;
            let price_final: f64 = price.unwrap() * porcentaje_adicional; 
            Some(price_final)
        } else {
            None
        };

        self.token_series_by_id.insert(&token_series_id, &TokenSeries{
            metadata: token_metadata.clone(),
            creator_id: caller_id.clone(),
            tokens: UnorderedSet::new(
                StorageKey::TokensBySeriesInner {
                    token_series: token_series_id.clone(),
                }
                .try_to_vec()
                .unwrap(),
            ),
            price: price_res,
            is_mintable: true,
            royalty: royalty_res.clone(),
        });

        

        env::log_str(
            &json!({
                "type": "nft_create_series",
                "params": {
                    "token_series_id": token_series_id.to_string(),
                    "token_metadata": token_metadata,
                    "creator_id": caller_id.to_string(),
                    "price": price_res,
                    "royalty": royalty_res
                }
            })
            .to_string(),
        );

        self.id_serie += 1;
        refund_deposit(env::storage_usage() - initial_storage_usage, 0);

		TokenSeriesJson{
            token_series_id,
			metadata: token_metadata,
			creator_id: caller_id.into(),
            royalty: royalty_res,
		}
    }



    #[payable]
    pub fn nft_buy(
        &mut self, 
        token_series_id: TokenSeriesId,
        receiver_id: Option<AccountId>
    ) -> TokenId {
        let initial_storage_usage = env::storage_usage();
        
        let token_series = self.token_series_by_id.get(&token_series_id).expect("Token series not exist");
        
        let price: f64 = token_series.price.expect("not for sale");
        let attached_deposit = env::attached_deposit();
        let receiver_id: AccountId = if let Some(receiver_id) = receiver_id {
            if !is_valid_account_id(receiver_id.as_bytes()) {
                env::panic_str("Not valid account_id for royalty");
            };
            receiver_id
        } else {
            env::predecessor_account_id()
        };
        

        let price_near: f64 = price / self.tasa;
        let price_yocto: u128 = (price_near * 10u128.pow(24) as f64) as u128;

        
        assert!(
            attached_deposit >= (price_yocto + 50_000_000_000_000_000_000_000u128),
            "attached deposit is less than price : {}",
            (price_yocto + 50_000_000_000_000_000_000_000u128)
        );

        let token_id: TokenId = self._nft_mint_series(token_series_id, receiver_id.clone());

        
        Promise::new(self.vault_id.clone()).transfer(price_yocto);
        
        
        refund_deposit(env::storage_usage() - initial_storage_usage, price_yocto);


        env::log_str(
            &json!({
                "type": "nft_buy",
                "params": { 
                    "tasa": self.tasa,
                    "price_usd": price.to_string(),
                    "price": price_yocto.to_string(),
                }
            })
            .to_string(),
        );

        token_id
        
    }

    #[payable]
    pub fn nft_mint(
        &mut self, 
        token_series_id: TokenSeriesId, 
        receiver_id: AccountId
    ) -> TokenId {
        let initial_storage_usage = env::storage_usage();

        let token_series = self.token_series_by_id.get(&token_series_id).expect("Token series not exist");
        assert_eq!(env::predecessor_account_id(), token_series.creator_id.clone(), "not creator");
        let token_id: TokenId = self._nft_mint_series(token_series_id, receiver_id.clone());

        refund_deposit(env::storage_usage() - initial_storage_usage, 0);

        token_id
    }


    #[payable]
    pub fn nft_mint_for(
        &mut self, 
        token_series_id: TokenSeriesId,
        nft_quantity: i64,
        receiver_id: AccountId
    ) -> i64 {
        let initial_storage_usage = env::storage_usage();

        let token_series = self.token_series_by_id.get(&token_series_id).expect("Token series not exist");
        assert!(nft_quantity <= 40, "No se pueden mintear mas de 40 nft en un solo llamado");
        assert!(env::predecessor_account_id() == token_series.creator_id.clone() || self.owner_id == env::signer_account_id() || self.list_admin.contains(&env::signer_account_id()), "Only administrator");
        
        
        for _i in 0..nft_quantity {
            self._nft_mint_series(token_series_id.clone(), receiver_id.clone());
        }

        
        refund_deposit(env::storage_usage() - initial_storage_usage, 0);

        nft_quantity
    }

    #[payable]
    pub fn deliver_gift(
        &mut self,
        receiver_id: AccountId
    ) {       
        let tokens_per_owner = self.tokens.tokens_per_owner.as_ref().expect(
            "Could not find tokens_per_owner when calling a method on the enumeration standard.",
        );
        let token_set = tokens_per_owner.get(&env::predecessor_account_id());

        assert!(token_set.is_some(), "No quedan Nft para regalar");
        
        let first_idx: usize = 0;
        let token_id:TokenId = token_set.unwrap().iter()
            .skip(0 as usize)
            .take(1)
            .map(|token_id| token_id).collect::<Vec<_>>()[first_idx].clone();

        self.tokens.nft_transfer(receiver_id.clone(), token_id , None, None);
        // self.tokens.nft_transfer(receiver_id.clone(), token_id.clone(), approval_id, memo.clone());
    }

    fn _nft_mint_series(
        &mut self, 
        token_series_id: TokenSeriesId, 
        receiver_id: AccountId
    ) -> TokenId {
        let mut token_series = self.token_series_by_id.get(&token_series_id).expect("Token series not exist");
    
        assert!(
            token_series.is_mintable,
            "Token series is not mintable"
        );

        let num_tokens = token_series.tokens.len();
        let max_copies = token_series.metadata.copies.unwrap_or(u64::MAX);
        
        assert!(num_tokens < max_copies, "Series supply maxed");

        if (num_tokens + 1) >= max_copies {
            token_series.is_mintable = false;
            token_series.price = None;
        }
        
        let token_id = format!("{}{}{}", &token_series_id, TOKEN_DELIMETER, num_tokens + 1);
        token_series.tokens.insert(&token_id);
        self.token_series_by_id.insert(&token_series_id, &token_series);
        let title: String = format!("{} {} {} {} {}", token_series.metadata.title.unwrap().clone(), TITLE_DELIMETER, token_series_id, TITLE_DELIMETER, (num_tokens + 1).to_string());
        
        let token_metadata = Some(TokenMetadata {
            title: Some(title),
            description: None,    // free-form description
            media: None, // URL to associated media, preferably to decentralized, content-addressed storage
            media_hash: None, // Base64-encoded sha256 hash of content referenced by the `media` field. Required if `media` is included.
            copies: None, // number of copies of this set of metadata in existence when token was minted.
            issued_at: Some(env::block_timestamp().to_string()), // ISO 8601 datetime when token was issued or minted
            expires_at: None, // ISO 8601 datetime when token expires
            starts_at: None, // ISO 8601 datetime when token starts being valid
            updated_at: None, // ISO 8601 datetime when token was last updated
            extra: None, // anything extra the NFT wants to store on-chain. Can be stringified JSON.
            reference: None, // URL to an off-chain JSON file with more info.
            reference_hash: None,
        });

        let token_owner_id: AccountId = receiver_id.clone();
      
        //self.tokens.internal_mint(token_id.clone(), token_owner_id, token_metadata);
        //let owner_id: AccountId = receiver_id.clone();
        self.tokens.owner_by_id.insert(&token_id, &token_owner_id);

        self.tokens
            .token_metadata_by_id
            .as_mut()
            .and_then(|by_id| by_id.insert(&token_id, &token_metadata.as_ref().unwrap()));

        if let Some(tokens_per_owner) = &mut self.tokens.tokens_per_owner {
            let mut token_ids = tokens_per_owner.get(&token_owner_id).unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokensPerOwner {
                    account_hash: env::sha256(&token_owner_id.as_bytes()),
                })
            });
            token_ids.insert(&token_id);
            tokens_per_owner.insert(&token_owner_id, &token_ids);
        }
        
        NearEvent::log_nft_mint(
            receiver_id.to_string(),
            vec![token_id.clone()],
            None,
        );

        token_id

    }



    #[payable]
    pub fn nft_burn(&mut self, token_id: TokenId) {
        assert_one_yocto();

        let owner_id = self.tokens.owner_by_id.get(&token_id).unwrap();
        
        assert_eq!(
            owner_id,
            env::predecessor_account_id(),
            "Token owner only"
        );

        if let Some(next_approval_id_by_id) = &mut self.tokens.next_approval_id_by_id {
            next_approval_id_by_id.remove(&token_id);
        }

        if let Some(approvals_by_id) = &mut self.tokens.approvals_by_id {
            approvals_by_id.remove(&token_id);
        }

        if let Some(tokens_per_owner) = &mut self.tokens.tokens_per_owner {
            let mut token_ids = tokens_per_owner.get(&owner_id).unwrap();
            token_ids.remove(&token_id);
            tokens_per_owner.insert(&owner_id, &token_ids);
        }

        if let Some(token_metadata_by_id) = &mut self.tokens.token_metadata_by_id {
            token_metadata_by_id.remove(&token_id);
        }

        self.tokens.owner_by_id.remove(&token_id);

        NearEvent::log_nft_burn(
            owner_id.to_string(),
            vec![token_id.clone()],
            None,
            None,
        );
    }


    pub fn get_nft_series_copies_availables(&self, token_series_id: TokenSeriesId) -> u64 {
		let token_series = self.token_series_by_id.get(&token_series_id).expect("Series does not exist");
        let copies_availables = token_series.metadata.copies.unwrap() - token_series.tokens.len() as u64 ;
        copies_availables    
	}

    /*pub fn get_nft_series_single(&self, token_series_id: TokenSeriesId) -> TokenSeriesJson {
		let token_series = self.token_series_by_id.get(&token_series_id).expect("Series does not exist");
		TokenSeriesJson{
            token_series_id,
			metadata: token_series.metadata,
			creator_id: token_series.creator_id,
            royalty: token_series.royalty,
		}
	}*/

    pub fn get_nft_series(
        &self,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<TokenSeriesJson2> {
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        assert!(
            (self.token_series_by_id.len() as u128) > start_index,
            "Out of bounds, please use a smaller from_index."
        );
        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        assert_ne!(limit, 0, "Cannot provide limit of 0.");

        self.token_series_by_id
            .iter()
            .skip(start_index as usize)
            .take(limit)
            .map(|(token_series_id, token_series)| {
                let mut price_yocto: Option<u128> = None;
                if token_series.price.is_some() {
                    let price_near: f64 = token_series.price.unwrap() / self.tasa;
                    price_yocto = Some((price_near * 10u128.pow(24) as f64) as u128);
                }
                
                TokenSeriesJson2 {
                    token_series_id: token_series_id.clone(),
                    metadata: token_series.metadata,
                    creator_id: token_series.creator_id,
                    price: price_yocto,
                    price_usd: token_series.price,
                    is_mintable: token_series.is_mintable,
                    royalty: token_series.royalty
                }
            })
            .collect()
    }


    /*pub fn nft_token(&self, token_id: TokenId) -> Option<TokenCustom> {
        let owner_id = self.tokens.owner_by_id.get(&token_id)?;
        let approved_account_ids = self
            .tokens
            .approvals_by_id
            .as_ref()
            .and_then(|by_id| by_id.get(&token_id).or_else(|| Some(HashMap::new())));

        let mut token_id_iter = token_id.split(TOKEN_DELIMETER);
        let token_series_id = token_id_iter.next().unwrap().parse().unwrap();
        let royalty = self.token_series_by_id.get(&token_series_id).unwrap().royalty;

        let token_metadata = self.tokens.token_metadata_by_id.as_ref().unwrap().get(&token_id).unwrap();

        Some(TokenCustom {
            token_id,
            owner_id,
            metadata: Some(token_metadata),
            approved_account_ids,
            royalty: Some(royalty)
        })
    }*/

    
    pub fn nft_token(&self, token_id: TokenId) -> Option<TokenCustom> {
        let owner_id = self.tokens.owner_by_id.get(&token_id)?;
        let approved_account_ids = self
            .tokens
            .approvals_by_id
            .as_ref()
            .and_then(|by_id| by_id.get(&token_id).or_else(|| Some(HashMap::new())));

        // CUSTOM (switch metadata for the token_series metadata)
        let mut token_id_iter = token_id.split(TOKEN_DELIMETER);
        let token_series_id = token_id_iter.next().unwrap().parse().unwrap();
        let data_token_serie_id = self.token_series_by_id.get(&token_series_id).unwrap();
        
        let series_metadata = data_token_serie_id.metadata;
        let royalty = data_token_serie_id.royalty;

        let mut token_metadata = self.tokens.token_metadata_by_id.as_ref().unwrap().get(&token_id).unwrap();

        token_metadata.reference = series_metadata.reference;
        token_metadata.description = series_metadata.description;
        token_metadata.media = series_metadata.media;
        token_metadata.copies = series_metadata.copies;
        token_metadata.extra = series_metadata.extra;

        Some(TokenCustom {
            token_id,
            owner_id,
            metadata: Some(token_metadata),
            approved_account_ids,
            royalty: Some(royalty)
        })
    }


    pub fn nft_transfer_unsafe(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        let sender_id = env::predecessor_account_id();
        let (previous_owner_id, _) = self.tokens.internal_transfer(&sender_id, &receiver_id, &token_id, approval_id, memo.clone());

        let authorized_id : Option<String> = if sender_id != previous_owner_id {
            Some(sender_id.to_string())
        } else {
            None
        };

        NearEvent::log_nft_transfer(
            previous_owner_id.to_string(),
            receiver_id.to_string(),
            vec![token_id],
            memo,
            authorized_id,
        );

        
    }

    #[payable]
    pub fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        self.tokens.nft_transfer(receiver_id.clone(), token_id.clone(), approval_id, memo.clone());
    }

    #[payable]
    pub fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let (previous_owner_id, old_approvals) = self.tokens.internal_transfer(
            &sender_id,
            &receiver_id.clone(),
            &token_id,
            approval_id,
            memo.clone(),
        );

        let authorized_id : Option<String> = if sender_id != previous_owner_id {
            Some(sender_id.to_string())
        } else {
            None
        };

        NearEvent::log_nft_transfer(
            previous_owner_id.to_string(),
            receiver_id.to_string(),
            vec![token_id.clone()],
            memo,
            authorized_id,
        );

        // Initiating receiver's call and the callback
        ext_non_fungible_token_receiver::ext(receiver_id.clone())
            .with_static_gas(env::prepaid_gas() - GAS_FOR_NFT_TRANSFER_CALL)
            .nft_on_transfer(
                sender_id,
                previous_owner_id.clone(),
                token_id.clone(),
                msg,
            ).then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .nft_resolve_transfer(
                        previous_owner_id,
                        receiver_id.into(),
                        token_id,
                        old_approvals,
                    )
            ).into()

        /*ext_non_fungible_token_receiver::nft_on_transfer(
            sender_id,
            previous_owner_id.clone(),
            token_id.clone(),
            msg,
            receiver_id.as_ref(),
            NO_DEPOSIT,
            env::prepaid_gas() - GAS_FOR_NFT_TRANSFER_CALL,
        )
        .then(ext_self::nft_resolve_transfer(
            previous_owner_id,
            receiver_id.into(),
            token_id,
            old_approvals,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
        .into()*/

    }

    // CUSTOM enumeration standard modified here because no macro below

    pub fn nft_total_supply(&self) -> U128 {
        (self.tokens.owner_by_id.len() as u128).into()
    }

    pub fn nft_supply_for_series(&self, token_series_id: TokenSeriesId) -> U64 {
        self.token_series_by_id.get(&token_series_id).expect("Token series not exist").tokens.len().into()
    }

    pub fn nft_get_series_single(&self, token_series_id: TokenSeriesId) -> TokenSeriesJson3 {
		let token_series = self.token_series_by_id.get(&token_series_id).expect("Series does not exist");
        
		TokenSeriesJson3{
            token_series_id,
			metadata: token_series.metadata,
			creator_id: token_series.creator_id,
            royalty: token_series.royalty,
            transaction_fee: U128::from(CURRENT_TRANSACTION_FEE)
		}
	}

    pub fn nft_get_series_price(self, token_series_id: TokenSeriesId) -> Option<U128> {
        let price = self.token_series_by_id.get(&token_series_id).unwrap().price;

        match price {
            Some(p) => {
                let price_near: f64 = p / self.tasa;
                let price_yocto: u128 = (price_near * 10u128.pow(24) as f64) as u128;
                return Some(U128::from(price_yocto + 100_000_000_000_000_000_000_000u128))
                //return Some(U128::from(price_yocto))
            },
            None => return None
        };
    }

    pub fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<TokenCustom> {
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        assert!(
            (self.tokens.owner_by_id.len() as u128) > start_index,
            "Out of bounds, please use a smaller from_index."
        );
        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        assert_ne!(limit, 0, "Cannot provide limit of 0.");
        self.tokens
            .owner_by_id
            .iter()
            .skip(start_index as usize)
            .take(limit)
            .map(|(token_id, _)| self.nft_token(token_id).unwrap())
            .collect()
    }

    pub fn nft_supply_for_owner(self, account_id: AccountId) -> U128 {
        let tokens_per_owner = self.tokens.tokens_per_owner.expect(
            "Could not find tokens_per_owner when calling a method on the enumeration standard.",
        );
        tokens_per_owner
            .get(&account_id)
            .map(|account_tokens| U128::from(account_tokens.len() as u128))
            .unwrap_or(U128(0))
    }

    pub fn is_member(self, account_id: AccountId) -> U128 {
        let tokens_per_owner = self.tokens.tokens_per_owner.as_ref().expect(
            "Could not find tokens_per_owner when calling a method on the enumeration standard.",
        );
        
        tokens_per_owner.get(&account_id).expect("account not member");

        (self.tokens.owner_by_id.len() as u128).into()
    }

    pub fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<TokenCustom> {
        let tokens_per_owner = self.tokens.tokens_per_owner.as_ref().expect(
            "Could not find tokens_per_owner when calling a method on the enumeration standard.",
        );
        let token_set = if let Some(token_set) = tokens_per_owner.get(&account_id) {
            token_set
        } else {
            return vec![];
        };
        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        assert_ne!(limit, 0, "Cannot provide limit of 0.");
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        assert!(
            token_set.len() as u128 > start_index,
            "Out of bounds, please use a smaller from_index."
        );
        token_set
            .iter()
            .skip(start_index as usize)
            .take(limit)
            .map(|token_id| self.nft_token(token_id).unwrap())
            .collect()
    }

    pub fn nft_payout(
        &self, 
        token_id: TokenId,
        balance: U128, 
        max_len_payout: u32
    ) -> Payout{
        let owner_id = self.tokens.owner_by_id.get(&token_id).expect("No token id");
        let mut token_id_iter = token_id.split(TOKEN_DELIMETER);
        let token_series_id = token_id_iter.next().unwrap().parse().unwrap();
        let royalty = self.token_series_by_id.get(&token_series_id).expect("no type").royalty;

        assert!(royalty.len() as u32 <= max_len_payout, "Market cannot payout to that many receivers");

        let balance_u128: u128 = balance.into();

        let mut payout: Payout = Payout { payout: HashMap::new() };
        let mut total_perpetual = 0;

        for (k, v) in royalty.iter() {
            if *k != owner_id {
                let key = k.clone();
                payout.payout.insert(key, royalty_to_payout(*v, balance_u128));
                total_perpetual += *v;
            }
        }
        payout.payout.insert(owner_id, royalty_to_payout(10000 - total_perpetual, balance_u128));
        payout
    }

    #[payable]
    pub fn nft_transfer_payout(
        &mut self, 
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        balance: Option<U128>,
        max_len_payout: Option<u32>
    ) -> Option<Payout> {
        assert_one_yocto();
        // Transfer
        let previous_token = self.nft_token(token_id.clone()).expect("no token");
        self.tokens.nft_transfer(receiver_id.clone(), token_id.clone(), approval_id, None);

        // Payout calculation
        let previous_owner_id = previous_token.owner_id;
        let mut total_perpetual = 0;
        let payout = if let Some(balance) = balance {
            let balance_u128: u128 = u128::from(balance);
            let mut payout: Payout = Payout { payout: HashMap::new() };

            let mut token_id_iter = token_id.split(TOKEN_DELIMETER);
            let token_series_id = token_id_iter.next().unwrap().parse().unwrap();
            let royalty = self.token_series_by_id.get(&token_series_id).expect("no type").royalty;

            assert!(royalty.len() as u32 <= max_len_payout.unwrap(), "Market cannot payout to that many receivers");
            for (k, v) in royalty.iter() {
                let key = k.clone();
                if key != previous_owner_id {
                    payout.payout.insert(key, royalty_to_payout(*v, balance_u128));
                    total_perpetual += *v;
                }
            }

            assert!(
                total_perpetual <= 10000,
                "Total payout overflow"
            );

            payout.payout.insert(previous_owner_id.clone(), royalty_to_payout(10000 - total_perpetual, balance_u128));
            Some(payout)
        } else {
            None
        };

        payout
    }



}


/* codigo costumizado */
fn royalty_to_payout(a: u32, b: Balance) -> U128 {
    U128(a as u128 * b / 10_000u128)
}


near_contract_standards::impl_non_fungible_token_approval!(Contract, tokens);

#[near_bindgen]
impl NonFungibleTokenMetadataProvider for Contract {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[near_bindgen]
impl NonFungibleTokenResolver for Contract {
    #[private]
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<HashMap<AccountId, u64>>,
    ) -> bool {
        let resp: bool = self.tokens.nft_resolve_transfer(
            previous_owner_id.clone(),
            receiver_id.clone(),
            token_id.clone(),
            approved_account_ids,
        );

        // if not successful, return nft back to original owner
        if !resp {
            NearEvent::log_nft_transfer(
                receiver_id.to_string(),
                previous_owner_id.to_string(),
                vec![token_id],
                None,
                None,
            );
        }

        resp
    }
}


fn refund_deposit(storage_used: u64, extra_spend: Balance) {
    let required_cost = env::storage_byte_cost() * Balance::from(storage_used);
    let attached_deposit = env::attached_deposit() - extra_spend;

    assert!(
        required_cost <= attached_deposit,
        "Must attach {} yoctoNEAR to cover storage",
        required_cost,
    );

    let refund = attached_deposit - required_cost;
    if refund > 1 {
        Promise::new(env::predecessor_account_id()).transfer(refund);
    }
}



/*----------- test --------------*/
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;
    use std::collections::HashMap;

    use super::*;

    const MINT_STORAGE_COST: u128 = 5870000000000000000000;

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    fn sample_token_metadata() -> TokenMetadata {
        TokenMetadata {
            title: Some("Olympus Mons".into()),
            description: Some("The tallest mountain in the charted solar system".into()),
            media: None,
            media_hash: None,
            copies: Some(1u64),
            issued_at: None,
            expires_at: None,
            starts_at: None,
            updated_at: None,
            extra: None,
            reference: None,
            reference_hash: None,
        }
    }

    #[test]
    fn test_new() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new_default_meta(accounts(1).into());
        testing_env!(context.is_view(true).build());
        assert_eq!(contract.nft_token("1".to_string()), None);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    fn test_mint() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());

        let token_id = "0".to_string();
        let token = contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());
        assert_eq!(token.token_id, token_id);
        assert_eq!(token.owner_id.to_string(), accounts(0).to_string());
        assert_eq!(token.metadata.unwrap(), sample_token_metadata());
        assert_eq!(token.approved_account_ids.unwrap(), HashMap::new());
    }

    #[test]
    fn test_transfer() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_transfer(accounts(1), token_id.clone(), None, None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        if let Some(token) = contract.nft_token(token_id.clone()) {
            assert_eq!(token.token_id, token_id);
            assert_eq!(token.owner_id.to_string(), accounts(1).to_string());
            assert_eq!(token.metadata.unwrap(), sample_token_metadata());
            assert_eq!(token.approved_account_ids.unwrap(), HashMap::new());
        } else {
            panic!("token not correctly created, or not found by nft_token");
        }
    }

    #[test]
    fn test_approve() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        // alice approves bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(150000000000000000000)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_approve(token_id.clone(), accounts(1), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert!(contract.nft_is_approved(token_id.clone(), accounts(1), Some(1)));
    }

    #[test]
    fn test_revoke() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        // alice approves bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(150000000000000000000)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_approve(token_id.clone(), accounts(1), None);

        // alice revokes bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_revoke(token_id.clone(), accounts(1));
        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert!(!contract.nft_is_approved(token_id.clone(), accounts(1), None));
    }

    #[test]
    fn test_revoke_all() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        // alice approves bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(150000000000000000000)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_approve(token_id.clone(), accounts(1), None);

        // alice revokes bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_revoke_all(token_id.clone());
        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert!(!contract.nft_is_approved(token_id.clone(), accounts(1), Some(1)));
    }
}