use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    sysvar::{
        rent::Rent
    },
    clock
};
use anchor_spl::token::{self, TokenAccount, Token, Mint};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

pub mod constants {
    pub const NFT_COLLATERIZED_LOANS_SEED: &[u8] = b"config";
    pub const NFT_COLLATERIZED_LOANS_ST_VAULT_SEED: &[u8] = b"st_vault";
    pub const NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED: &[u8] = b"nft_vault";
}

#[program]
pub mod nft_loans {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, _fee_pt: u8) -> Result<()> {
        let configuration = &mut ctx.accounts.configuration;
        configuration.stablecoin_mint = ctx.accounts.stablecoin_mint.key();
        configuration.stablecoin_vault = ctx.accounts.stablecoin_vault.key();
        configuration.order_id = 0;
        configuration.total_additional_collateral = 0;
        configuration.fee_pt = _fee_pt;
        Ok(())
    }

    // create_order
    pub fn create_order(ctx: Context<CreateOrder>, _request_amount: u64, _interest: u64, _period: u64, _additional_collateral: u64) -> Result<()> {
        if _request_amount == 0 {
            return Err(ErrorCode::AmountMustBeGreaterThanZero.into());
        }

        // Transfer collateral to vault.
        {
            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.user_nft_vault.to_account_info(),
                    to: ctx.accounts.nft_vault.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(), //todo use user account as signer
                },
            );
            token::transfer(cpi_ctx, 1)?;
        }

        // Transfer additional collateral to vault
        {
            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.user_stablecoin_vault.to_account_info(),
                    to: ctx.accounts.stablecoin_vault.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(), //todo use user account as signer
                },
            );
            token::transfer(cpi_ctx, _additional_collateral)?;
        }

        let clock = clock::Clock::get().unwrap();

        // Save Info
        let order = &mut ctx.accounts.order;
        order.borrower = ctx.accounts.signer.key();
        order.stablecoin_vault = ctx.accounts.stablecoin_vault.key();
        order.nft_mint = ctx.accounts.nft_mint.key();
        order.nft_vault = ctx.accounts.nft_vault.key();
        order.request_amount = _request_amount;
        order.interest = _interest;
        order.period = _period;
        order.additional_collateral = _additional_collateral;
        order.lender = order.key(); // just a placeholder
        order.created_at = clock.unix_timestamp as u64;
        order.loan_start_time = 0; // placeholder
        order.paid_back_at = 0;
        order.withdrew_at = 0;

        let nft_collaterized_loans = &mut ctx.accounts.configuration;
        nft_collaterized_loans.total_additional_collateral += _additional_collateral;

        nft_collaterized_loans.order_id += 1;

        order.order_status = true;

        Ok(())
    }

    pub fn cancel_order(ctx: Context<CancelOrder>, _order_id: u64) -> Result<()> {
        let order = &mut ctx.accounts.order;
        let configuration = &mut ctx.accounts.configuration;

        if order.loan_start_time != 0 && order.order_status == false {
            return Err(ErrorCode::LoanAlreadyStarted.into());
        }

        let nonce = *(ctx.bumps.get("nft_vault").unwrap());

        // Transfer back nft collateral.
        {
            let seeds = &[ctx.accounts.nft_mint.to_account_info().key.as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref(), &[nonce]];
            let signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.nft_vault.to_account_info(),
                    to: ctx.accounts.user_nft_vault.to_account_info(),
                    authority: ctx.accounts.nft_vault.to_account_info(),
                },
                signer
            );
            token::transfer(cpi_ctx, 1)?;
        }

        let nonce = *(ctx.bumps.get("stablecoin_vault").unwrap());

        // Transfer back additional collateral
        {
            let seeds = &[ctx.accounts.stablecoin_mint.to_account_info().key.as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref(), &[nonce]];
            let signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.stablecoin_vault.to_account_info(),
                    to: ctx.accounts.user_stablecoin_vault.to_account_info(),
                    authority: ctx.accounts.stablecoin_vault.to_account_info(),
                },
                signer
            );

            token::transfer(cpi_ctx, order.additional_collateral)?;
        }
        configuration.total_additional_collateral -= order.additional_collateral;

        Ok(())
    }

    pub fn give_loan(ctx: Context<GiveLoan>, _order_id: u64) -> Result<()> {
        let order = &mut ctx.accounts.order;

        if order.loan_start_time != 0 && order.order_status == false {
            return Err(ErrorCode::LoanAlreadyStarted.into());
        }

        // Transfer back additional collateral
        {
            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.lender_stablecoin_vault.to_account_info(),
                    to: ctx.accounts.borrower_stablecoin_vault.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            );
            token::transfer(cpi_ctx, order.request_amount)?;
        }

        // Save Info
        order.lender = ctx.accounts.signer.key();
        order.loan_start_time = clock::Clock::get().unwrap().unix_timestamp as u64;
        order.order_status = false;

        Ok(())
    }

    pub fn payback(ctx: Context<Payback>, _order_id: u64) -> Result<()> {
        let order = &mut ctx.accounts.order;
        let configuration = &mut ctx.accounts.configuration;

        if order.loan_start_time == 0 && order.order_status == true {
            return Err(ErrorCode::LoanNotProvided.into());
        }

        let clock = clock::Clock::get().unwrap();
        if order.loan_start_time.checked_add(order.period).unwrap() < clock.unix_timestamp as u64 {
            return Err(ErrorCode::RepaymentPeriodExceeded.into());
        }

        // Save Info
        order.paid_back_at = clock.unix_timestamp as u64;

        // Pay Loan
        {
            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.user_stablecoin_vault.to_account_info(),
                    to: ctx.accounts.lender_stablecoin_vault.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            );
            token::transfer(cpi_ctx, order.request_amount.checked_add(order.interest).unwrap())?;
        }

        let nonce = *(ctx.bumps.get("nft_vault").unwrap());
        // Transfer back nft collateral.
        {
            let seeds = &[ctx.accounts.nft_mint.to_account_info().key.as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref(), &[nonce]];
            let signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.nft_vault.to_account_info(),
                    to: ctx.accounts.user_nft_vault.to_account_info(),
                    authority: ctx.accounts.nft_vault.to_account_info(),
                },
                signer
            );
            token::transfer(cpi_ctx, 1)?;
        }

        let nonce = *(ctx.bumps.get("stablecoin_vault").unwrap());
        // Transfer back additional collateral
        {
            let seeds = &[ctx.accounts.stablecoin_mint.to_account_info().key.as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref(), &[nonce]];
            let signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.stablecoin_vault.to_account_info(),
                    to: ctx.accounts.user_stablecoin_vault.to_account_info(),
                    authority: ctx.accounts.stablecoin_vault.to_account_info(),
                },
                signer
            );
            token::transfer(cpi_ctx, order.additional_collateral)?;
        }
        configuration.total_additional_collateral -= order.additional_collateral;

        Ok(())
    }

    pub fn liquidate(ctx: Context<Liquidate>, _order_id: u64) -> Result<()> {
        let order = &mut ctx.accounts.order;
        let configuration = &mut ctx.accounts.configuration;

        if order.loan_start_time == 0 && order.order_status == true {
            return Err(ErrorCode::LoanNotProvided.into());
        }

        let clock = clock::Clock::get().unwrap();
        if order.loan_start_time.checked_add(order.period).unwrap() > clock.unix_timestamp as u64 {
            return Err(ErrorCode::RepaymentPeriodNotExceeded.into());
        }

        if order.withdrew_at != 0 {
            return Err(ErrorCode::AlreadyLiquidated.into());
        }

        // Save Info
        order.withdrew_at = clock.unix_timestamp as u64;

        let nonce = *(ctx.bumps.get("nft_vault").unwrap());
        // Transfer nft collateral.
        {
            let seeds = &[ctx.accounts.nft_mint.to_account_info().key.as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref(), &[nonce]];
            let signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.nft_vault.to_account_info(),
                    to: ctx.accounts.user_nft_vault.to_account_info(),
                    authority: ctx.accounts.nft_vault.to_account_info(),
                },
                signer
            );
            token::transfer(cpi_ctx, 1)?;
        }

        let nonce = *(ctx.bumps.get("stablecoin_vault").unwrap());
        // Transfer additional collateral
        {
            let seeds = &[ctx.accounts.stablecoin_mint.to_account_info().key.as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref(), &[nonce]];
            let signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.stablecoin_vault.to_account_info(),
                    to: ctx.accounts.lender_stablecoin_vault.to_account_info(),
                    authority: ctx.accounts.stablecoin_vault.to_account_info(),
                },
                signer
            );
            token::transfer(cpi_ctx, order.additional_collateral)?;
        }
        configuration.total_additional_collateral -= order.additional_collateral;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub stablecoin_mint: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = signer,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref()],
        bump,
        token::mint = stablecoin_mint,
        token::authority = stablecoin_vault,
    )]
    pub stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = signer,
        space = 8 + Configuration::LEN,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_SEED.as_ref()],
        bump,
    )]
    pub configuration: Box<Account<'info, Configuration>>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct CreateOrder<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_SEED.as_ref()],
        bump,
    )]
    pub configuration: Box<Account<'info, Configuration>>,

    pub stablecoin_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref()],
        bump,
    )]
    pub stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = user_stablecoin_vault.owner == signer.key(),
    )]
    pub user_stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = nft_mint.supply == 1,
        constraint = nft_mint.decimals == 0,
    )]
    pub nft_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        constraint = user_nft_vault.mint == nft_mint.key(),
        constraint = user_nft_vault.owner == signer.key(),
    )]
    pub user_nft_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer=signer,
        seeds = [nft_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref()],
        bump,
        token::mint=nft_mint,
        token::authority = nft_vault,
    )]
    pub nft_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = signer,
        seeds = [
        configuration.order_id.to_string().as_ref(),
        configuration.to_account_info().key().as_ref()
        ],
        space = 8 + Order::LEN,
        bump,
    )]
    pub order: Box<Account<'info, Order>>,

    // misc
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(_order_id: u64)]
pub struct CancelOrder<'info> {
    #[account(
        mut,
        has_one = stablecoin_vault,
        has_one = stablecoin_mint
    )]
    pub configuration: Box<Account<'info, Configuration>>,

    // Order.
    #[account(
        mut,
        seeds = [
            _order_id.to_string().as_ref(),
            configuration.to_account_info().key().as_ref()
        ],
        bump,
        constraint = order.stablecoin_vault == stablecoin_vault.key(),
        constraint = order.nft_vault == nft_vault.key(),
        constraint = order.nft_mint == nft_mint.key(),
        close = signer,
    )]
    pub order: Box<Account<'info, Order>>,

    pub stablecoin_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref()],
        bump,
    )]
    pub stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = user_stablecoin_vault.owner == signer.key(),
    )]
    pub user_stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = nft_mint.supply == 1,
        constraint = nft_mint.decimals == 0,
    )]
    pub nft_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [nft_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref()],
        bump,
    )]
    pub nft_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_nft_vault.mint == nft_mint.key(),
        constraint = user_nft_vault.owner == signer.key(),
    )]
    pub user_nft_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut
    )]
    pub signer: Signer<'info>,

    // misc
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>
}

#[derive(Accounts)]
#[instruction(_order_id: u64)]
pub struct GiveLoan<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        has_one = stablecoin_vault,
        has_one = stablecoin_mint
    )]
    pub configuration: Box<Account<'info, Configuration>>,

    // Order.
    #[account(
        mut,
        constraint = order.stablecoin_vault == stablecoin_vault.key(),
        seeds = [
            _order_id.to_string().as_ref(),
            configuration.to_account_info().key().as_ref()
        ],
        bump,
    )]
    pub order: Box<Account<'info, Order>>,

    pub stablecoin_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref()],
        bump,
    )]
    pub stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = lender_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = lender_stablecoin_vault.owner == signer.key(),
    )]
    pub lender_stablecoin_vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = borrower_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = borrower_stablecoin_vault.owner == order.borrower,
    )]
    pub borrower_stablecoin_vault: Box<Account<'info, TokenAccount>>,

    // misc
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>
}

#[derive(Accounts)]
#[instruction(_order_id: u64)]
pub struct Payback<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        has_one = stablecoin_vault,
        has_one = stablecoin_mint
    )]
    pub configuration: Box<Account<'info, Configuration>>,

    // Order.
    #[account(
        mut,
        constraint = order.stablecoin_vault == stablecoin_vault.key(),
        constraint = order.nft_vault == nft_vault.key(),
        constraint = order.nft_mint == nft_mint.key(),
        seeds = [
            _order_id.to_string().as_ref(),
            configuration.to_account_info().key().as_ref()
        ],
        bump,
        close = signer,
    )]
    pub order: Box<Account<'info, Order>>,

    pub stablecoin_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref()],
        bump
    )]
    pub stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = lender_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = lender_stablecoin_vault.owner == order.lender,
    )]
    pub lender_stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = user_stablecoin_vault.owner == signer.key(),
    )]
    pub user_stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = nft_mint.supply == 1,
        constraint = nft_mint.decimals == 0,
    )]
    pub nft_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [nft_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref()],
        bump
    )]
    pub nft_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_nft_vault.mint == nft_mint.key(),
        constraint = user_nft_vault.owner == signer.key(),
    )]
    pub user_nft_vault: Box<Account<'info, TokenAccount>>,

    // misc
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>
}

#[derive(Accounts)]
#[instruction(_order_id: u64)]
pub struct Liquidate<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        has_one = stablecoin_vault,
        has_one = stablecoin_mint
    )]
    pub configuration: Box<Account<'info, Configuration>>,

    #[account(
        mut,
        seeds = [
            _order_id.to_string().as_ref(),
            configuration.to_account_info().key().as_ref()
        ],
        bump,
        constraint = order.stablecoin_vault == stablecoin_vault.key(),
        constraint = order.nft_vault == nft_vault.key(),
        constraint = order.nft_mint == nft_mint.key(),
        close = signer
    )]
    pub order: Box<Account<'info, Order>>,

    pub stablecoin_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [stablecoin_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_ST_VAULT_SEED.as_ref()],
        bump
    )]
    pub stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = lender_stablecoin_vault.mint == stablecoin_mint.key(),
        constraint = lender_stablecoin_vault.owner == order.lender,
    )]
    pub lender_stablecoin_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = nft_mint.supply == 1,
        constraint = nft_mint.decimals == 0,
    )]
    pub nft_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [nft_mint.key().as_ref(), constants::NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED.as_ref()],
        bump
    )]
    pub nft_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_nft_vault.mint == nft_mint.key(),
        constraint = user_nft_vault.owner == order.lender,
    )]
    pub user_nft_vault: Box<Account<'info, TokenAccount>>,

    // misc
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>
}

#[account]
pub struct Configuration {
    // Mint of the token
    pub stablecoin_mint: Pubkey,
    // Vault holding the stablecoins -- mostly for holding the collateral stablecoins
    pub stablecoin_vault: Pubkey,
    // last order id
    pub order_id: u64,
    // total additional collateral
    pub total_additional_collateral: u64,
    // fee percentage
    pub fee_pt: u8,
}

impl Configuration {
    pub const LEN:usize = 32 + 32 + 8 + 8 + 1;
}

#[account]
#[derive(Default)]
pub struct Order {
    // person requesting the loan
    pub borrower: Pubkey,
    /// vault to send the loan
    pub stablecoin_vault: Pubkey,
    // mint of the nft
    pub nft_mint: Pubkey,
    /// collateral vault holding the nft
    pub nft_vault: Pubkey,
    // request amount
    pub request_amount: u64,
    // interest amount
    pub interest: u64,
    // the loan period
    pub period: u64,
    // additional collateral
    pub additional_collateral: u64,
    // lender
    pub lender: Pubkey,
    // order created at
    pub created_at: u64,
    // loan start time
    pub loan_start_time: u64,
    // repayment timestamp
    pub paid_back_at: u64,
    // time the lender liquidated the loan & withdrew the collateral
    pub withdrew_at: u64,
    // status of the order
    pub order_status: bool,
}

impl Order {
    pub const LEN:usize = 32 * 4 + 8 * 4 + 32 + 8 * 4 + 1;
}

#[error_code]
pub enum ErrorCode {
    #[msg("Amount must be greater than zero.")]
    AmountMustBeGreaterThanZero,
    #[msg("Loan has started or already been canceled")]
    LoanAlreadyStarted,
    #[msg("Loan not provided yet")]
    LoanNotProvided,
    #[msg("Repayment Period has been exceeded")]
    RepaymentPeriodExceeded,
    #[msg("Repayment Period has not been exceeded")]
    RepaymentPeriodNotExceeded,
    #[msg("Already liquidated")]
    AlreadyLiquidated,
}