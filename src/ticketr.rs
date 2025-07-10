#![no_std]

extern crate alloc; // <<--- ¡¡AQUÍ ESTÁ LA LÍNEA MÁGICA!!

multiversx_sc::imports!();

// Las importaciones que ya habíamos descubierto que eran correctas
use multiversx_sc::codec;
use multiversx_sc::proxy_imports::{type_abi, TopDecode, TopEncode};
// AÑADIDO: Importamos el trait ToString para poder usarlo en números
use alloc::string::ToString;

// --- CONSTANTES ---
const TOKEN_TICKER: &[u8] = b"EVENTTICKET";
const AMOUNT_TO_MINT: u64 = 1;
const ROYALTIES: u32 = 0;

// --- ESTRUCTURA DE DATOS ---
#[type_abi]
#[derive(TopEncode, TopDecode)]
pub struct EventInfo<M: ManagedTypeApi> {
    pub name: ManagedBuffer<M>,
    pub total_tickets: u64,
    pub sold_tickets: u64,
}

// --- TODA LA LÓGICA DEL CONTRATO VA DENTRO DE ESTE ÚNICO TRAIT ---
#[multiversx_sc::contract]
pub trait TicketSaleContract {
    // --- STORAGE MAPPERS ---
    #[storage_mapper("eventInfo")]
    fn event_info(&self) -> SingleValueMapper<EventInfo<Self::Api>>;

    #[storage_mapper("ticketPrice")]
    fn ticket_price(&self) -> SingleValueMapper<BigUint>;

    #[storage_mapper("nftTokenIdentifier")]
    fn nft_token_identifier(&self) -> SingleValueMapper<TokenIdentifier>;

    // --- FUNCIÓN DE INICIALIZACIÓN ---
    #[init]
    fn init(&self) {}

    // --- ENDPOINTS ---
    #[endpoint]
    #[payable("EGLD")]
    fn setup_event(&self, event_name: ManagedBuffer, price: BigUint, total_tickets: u64) {
        self.blockchain().check_caller_is_owner();
        require!(self.nft_token_identifier().is_empty(), "Event already set up");
        require!(price > 0, "Price must be greater than 0");
        require!(total_tickets > 0, "Total tickets must be greater than 0");

        self.ticket_price().set(price);
        let event_data = EventInfo {
            name: event_name.clone(),
            total_tickets,
            sold_tickets: 0,
        };
        self.event_info().set(event_data);

        let issue_cost = self.call_value().egld();
        self.send()
            .esdt_system_sc_proxy()
            .issue_semi_fungible(
                issue_cost.clone_value(),
                &ManagedBuffer::new_from_bytes(TOKEN_TICKER),
                &ManagedBuffer::new_from_bytes(TOKEN_TICKER),
                SemiFungibleTokenProperties {
                    can_freeze: true,
                    can_wipe: true,
                    can_transfer_create_role: true,
                    ..Default::default()
                },
            )
            .with_callback(self.callbacks().issue_callback())
            .call_and_exit();
    }

    #[callback]
    fn issue_callback(&self, #[call_result] result: ManagedAsyncCallResult<TokenIdentifier>) {
        self.blockchain().check_caller_is_owner();
        match result {
            ManagedAsyncCallResult::Ok(token_id) => {
                self.nft_token_identifier().set(&token_id);
            }
            ManagedAsyncCallResult::Err(_) => {
                let owner = self.blockchain().get_owner_address();
                let returned_tokens = self.call_value().all_esdt_transfers();
                for payment in returned_tokens.iter() {
                    self.send().direct_esdt(
                        &owner,
                        &payment.token_identifier,
                        payment.token_nonce,
                        &payment.amount,
                    );
                }
                self.ticket_price().clear();
                self.event_info().clear();
            }
        }
    }

    #[endpoint]
    fn withdraw(&self) {
        self.blockchain().check_caller_is_owner();
        let owner_address = self.blockchain().get_owner_address();
        let sc_balance = self.blockchain().get_sc_balance(&EgldOrEsdtTokenIdentifier::egld(), 0);
        require!(sc_balance > 0, "No funds to withdraw");
        self.send().direct_egld(&owner_address, &sc_balance);
    }

    #[endpoint]
    #[payable("EGLD")]
    fn buy_ticket(&self) {
        require!(!self.nft_token_identifier().is_empty(), "Event is not set up yet");

        let payment = self.call_value().egld();
        let ticket_price = self.ticket_price().get();
        let caller = self.blockchain().get_caller();
        
        require!(*payment == ticket_price, "Payment amount does not match ticket price");

        let mut event_data = self.event_info().get();
        require!(event_data.sold_tickets < event_data.total_tickets, "All tickets have been sold");

        event_data.sold_tickets += 1;
        
        let token_id = self.nft_token_identifier().get();
        
        let mut nft_name = event_data.name.clone();
        nft_name.append_bytes(b" Ticket #");
        // Con `extern crate alloc;` y `use alloc::string::ToString;` esto ahora funciona.
        nft_name.append(&ManagedBuffer::from(event_data.sold_tickets.to_string().as_bytes()));

        let mut attributes = ManagedBuffer::new();
        attributes.append_bytes(b"type:ticket;event:");
        attributes.append(&event_data.name);
        
        let mut uris = ManagedVec::new();
        uris.push(ManagedBuffer::from(b"https://drive.google.com/file/d/1d5RbY7tUg3MT8ryb1pgKtSoi1LjUlyCa/view?usp=sharing".as_slice()));

        self.event_info().set(&event_data);

        // Paso 1: Crear el SFT. Esto devuelve el nuevo nonce.
        let new_nonce = self.send().esdt_nft_create(
            &token_id,
            &BigUint::from(AMOUNT_TO_MINT),
            &nft_name,
            &BigUint::from(ROYALTIES),
            &ManagedBuffer::new(), 
            &attributes,
            &uris,
        );

        // Paso 2: Enviar el SFT recién creado al comprador.
        self.send().direct_esdt(&caller, &token_id, new_nonce, &BigUint::from(AMOUNT_TO_MINT));
    }
    
    // --- VIEW FUNCTIONS ---
    #[view(getEventInfo)]
    fn get_event_info(&self) -> EventInfo<Self::Api> {
        self.event_info().get()
    }

    #[view(getTicketPrice)]
    fn get_ticket_price(&self) -> BigUint {
        self.ticket_price().get()
    }

    #[view(getNftTokenIdentifier)]
    fn get_nft_token_identifier(&self) -> TokenIdentifier {
        self.nft_token_identifier().get()
    }
}