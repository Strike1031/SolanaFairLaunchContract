// 1. Import dependencies
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{
        create_metadata_accounts_v3, mpl_token_metadata::types::DataV2, CreateMetadataAccountsV3,
        Metadata as Metaplex,
    },
    token::{self, mint_to, Mint, MintTo, Token, TokenAccount},
};
use solana_program::program::invoke;
use solana_program::system_instruction::transfer;

// 2. Declare Program ID (SolPG will automatically update this when you deploy)
declare_id!("2zP1cXE8o6dBDuNwfxoHgydP7ufn5sBibShiuv86RJ5b");

pub const GLOBAL_INFO_SEED: &str = "global_info";
pub const TOKEN_POOL_SEED: &str = "token_pool";
pub const SOL_VAULT_SEED: &str = "sol_escrow_seed";
pub const MINT_SEED: &str = "mint";

pub const GLOBAL_INFO_SIZE: usize = 8 + std::mem::size_of::<GlobalInfo>() + 8;
pub const TOKEN_POOL_SIZE: usize = 8 + std::mem::size_of::<TokenPools>() + 8;

pub fn calculate_fee(amount: u64, fee_percent: u32) -> u64 {
    (amount * fee_percent as u64) / 10000
}

pub fn get_price(sol_reserve: u64, token_reserve: u64) -> u64 {
    sol_reserve * 1e9 as u64 / token_reserve
}
// 3. Define the program and instructions
#[program]
mod token_minter {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.global_info.fee_percent = 300; // 1% = 100
        ctx.accounts.global_info.target_market_cap = 69000;
        ctx.accounts.global_info.target_lp_amount = 12000;
        ctx.accounts.global_info.total_supply = 1e18 as u64;
        ctx.accounts.global_info.initial_amount = 20e9 as u64;
        ctx.accounts.global_info.owner = ctx.accounts.owner.key();
        Ok(())
    }

    pub fn create_token(
        ctx: Context<InitToken>,
        metadata: InitTokenParams,
        amount: u64,
    ) -> Result<()> {
        require!(
            amount < ctx.accounts.global_info.initial_amount,
            CustomError::InvalidInitialValue
        );
        ctx.accounts.token_pools.sol_reserve = ctx.accounts.global_info.initial_amount;
        ctx.accounts.token_pools.token_reserve = ctx.accounts.global_info.total_supply;

        let name = metadata.name.clone();
        let seeds = &[MINT_SEED.as_bytes(), name.as_bytes(), &[ctx.bumps.mint]];
        let signer = [&seeds[..]];

        let token_data: DataV2 = DataV2 {
            name: metadata.name,
            symbol: metadata.symbol,
            uri: metadata.uri,
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        };

        let metadata_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountsV3 {
                payer: ctx.accounts.payer.to_account_info(),
                update_authority: ctx.accounts.mint.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                metadata: ctx.accounts.metadata.to_account_info(),
                mint_authority: ctx.accounts.mint.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            &signer,
        );

        create_metadata_accounts_v3(metadata_ctx, token_data, false, true, None)?;

        // Transfer SOL from buyer to contract account
        let transfer_instruction = transfer(
            &ctx.accounts.payer.key(),
            &ctx.accounts.escrow_account.key(),
            amount,
        );
        invoke(
            &transfer_instruction,
            &[
                ctx.accounts.payer.to_account_info(),
                ctx.accounts.escrow_account.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        let buy_fee: u64 = calculate_fee(amount, ctx.accounts.global_info.fee_percent);
        let effective_sol: u64 = amount - buy_fee;
        let token_price: u64 = get_price(
            ctx.accounts.token_pools.sol_reserve,
            ctx.accounts.token_pools.token_reserve,
        );
        let token_amount: u64 = (effective_sol * 1e9 as u64) / token_price;

        msg!("Token mint created successfully.");

        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    authority: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                },
                &signer,
            ),
            ctx.accounts.global_info.total_supply - token_amount,
        )?;

        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    authority: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.destination.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                },
                &signer,
            ),
            token_amount,
        )?;

        ctx.accounts.token_pools.token_reserve =
            ctx.accounts.global_info.total_supply - token_amount;
        ctx.accounts.token_pools.launched = 0;
        ctx.accounts.global_info.token_count += 1;

        Ok(())
    }

    pub fn buy_token(ctx: Context<BuyToken>, amount: u64) -> Result<()> {
        let buy_fee: u64 = calculate_fee(amount, ctx.accounts.global_info.fee_percent);
        let effective_sol: u64 = amount - buy_fee;
        let token_price: u64 = get_price(
            ctx.accounts.token_pools.sol_reserve,
            ctx.accounts.token_pools.token_reserve,
        );
        let token_amount: u64 = (effective_sol * 1e9 as u64) / token_price;
        // Transfer SOL from buyer to contract account
        let transfer_instruction = transfer(
            &ctx.accounts.buyer.key(),
            &ctx.accounts.escrow_account.key(),
            amount,
        );
        invoke(
            &transfer_instruction,
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.escrow_account.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        require!(
            ctx.accounts.token_pools.token_reserve > token_amount,
            CustomError::InvalidTokenAmount
        );

        let binding = ctx.accounts.mint.key();
        let seeds = &[binding.as_ref(), &[ctx.bumps.token_vault]];
        let signer_seeds = &[&seeds[..]];

        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.token_vault.to_account_info(),
                to: ctx.accounts.buyer_token_account.to_account_info(),
                authority: ctx.accounts.token_vault.to_account_info(),
            },
            signer_seeds,
        );
        token::transfer(transfer_ctx, token_amount)?;

        ctx.accounts.token_pools.sol_reserve += amount;
        ctx.accounts.token_pools.token_reserve -= token_amount;

        Ok(())
    }

    pub fn sell_token(ctx: Context<SellToken>, token_amount: u64) -> Result<()> {
        let sell_fee: u64 = calculate_fee(token_amount, ctx.accounts.global_info.fee_percent);
        let effective_token_amount: u64 = token_amount - sell_fee;
        let token_price: u64 = get_price(
            ctx.accounts.token_pools.sol_reserve,
            ctx.accounts.token_pools.token_reserve,
        );
        let sol_amount: u64 = (effective_token_amount * token_price) / 1e9 as u64;
        // Transfer tokens from seller to contract account

        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.seller_token_account.to_account_info(),
                to: ctx.accounts.token_vault.to_account_info(),
                authority: ctx.accounts.seller.to_account_info(),
            },
        );
        token::transfer(cpi_context, token_amount)?;

        require!(
            ctx.accounts.token_pools.sol_reserve > sol_amount,
            CustomError::InvalidSolAmount
        );
        // Transfer SOL from contract account to seller
        **ctx
            .accounts
            .escrow_account
            .to_account_info()
            .try_borrow_mut_lamports()? -= sol_amount;
        **ctx
            .accounts
            .seller
            .to_account_info()
            .try_borrow_mut_lamports()? += sol_amount;

        ctx.accounts.token_pools.sol_reserve -= sol_amount;
        ctx.accounts.token_pools.token_reserve += token_amount;

        Ok(())
    }

    pub fn add_liquidity(ctx: Context<AddLiquidity>, sol_price: u64) -> Result<()> {
        let init_coin_amount =
            ctx.accounts.global_info.target_lp_amount * 1e9 as u64 * 1000 / sol_price;
        let init_pc_amount = ctx.accounts.token_pools.token_reserve
            / ctx.accounts.token_pools.sol_reserve * init_coin_amount;

        let pda_account = ctx.accounts.escrow_account.to_account_info();
        let send_to_account = ctx.accounts.user_token_pc.to_account_info();

        let binding = ctx.accounts.token_vault.mint.key();
        let seeds = &[binding.as_ref(), &[ctx.bumps.token_vault]];
        let signer_seeds = &[&seeds[..]];

        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.token_vault.to_account_info(),
                to: ctx.accounts.user_token_coin.to_account_info(),
                authority: ctx.accounts.token_vault.to_account_info(),
            },
            signer_seeds,
        );
        
        token::transfer(transfer_ctx, init_pc_amount)?;

        **pda_account.try_borrow_mut_lamports()? -= init_coin_amount;
        **send_to_account.try_borrow_mut_lamports()? += init_coin_amount;

        // token::sync_native(CpiContext::new(
        //     ctx.accounts.token_program.to_account_info(),
        //     token::SyncNative {
        //         account: ctx.accounts.user_token_pc.to_account_info(),
        //     },
        // ))?;

        Ok(())
    }

    pub fn withdraw_balance(ctx: Context<WithdrawBalance>, amount: u64) -> Result<()> {
        require!(
            ctx.accounts.global_info.owner == ctx.accounts.admin.key(),
            CustomError::NotOwner
        );
        let pool_amount = **ctx.accounts.escrow_account.lamports.borrow();
        require!(pool_amount >= amount, CustomError::InvalidSolAmount);
        **ctx.accounts.escrow_account.try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.admin.try_borrow_mut_lamports()? += amount;

        ctx.accounts.token_pools.sol_reserve -= amount;
        Ok(())
    }

    pub fn set_fee_percent(ctx: Context<CommonCtx>, fee_percent: u32) -> Result<()> {
        require!(
            ctx.accounts.global_info.owner == ctx.accounts.admin.key(),
            CustomError::NotOwner
        );
        ctx.accounts.global_info.fee_percent = fee_percent;
        Ok(())
    }

    pub fn set_target_market_cap(ctx: Context<CommonCtx>, target_market_cap: u64) -> Result<()> {
        require!(
            ctx.accounts.global_info.owner == ctx.accounts.admin.key(),
            CustomError::NotOwner
        );
        ctx.accounts.global_info.target_market_cap = target_market_cap;
        Ok(())
    }

    pub fn set_target_lp_amount(ctx: Context<CommonCtx>, target_lp_amount: u64) -> Result<()> {
        require!(
            ctx.accounts.global_info.owner == ctx.accounts.admin.key(),
            CustomError::NotOwner
        );
        ctx.accounts.global_info.target_lp_amount = target_lp_amount;
        Ok(())
    }

    pub fn set_total_supply(ctx: Context<CommonCtx>, total_supply: u64) -> Result<()> {
        require!(
            ctx.accounts.global_info.owner == ctx.accounts.admin.key(),
            CustomError::NotOwner
        );
        ctx.accounts.global_info.total_supply = total_supply;
        Ok(())
    }

    pub fn set_initial_amount(ctx: Context<CommonCtx>, initial_amount: u64) -> Result<()> {
        require!(
            ctx.accounts.global_info.owner == ctx.accounts.admin.key(),
            CustomError::NotOwner
        );
        ctx.accounts.global_info.initial_amount = initial_amount;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init_if_needed,
        payer = owner,
        seeds = [GLOBAL_INFO_SEED.as_bytes()],
        bump,
        space = GLOBAL_INFO_SIZE
    )]
    pub global_info: Account<'info, GlobalInfo>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(
    params: InitTokenParams
)]
pub struct InitToken<'info> {
    /// CHECK: New Metaplex Account being created
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,
    #[account(
        init,
        seeds = [MINT_SEED.as_bytes(), params.name.as_bytes()],
        bump,
        payer = payer,
        mint::decimals = params.decimals,
        mint::authority = mint,
    )]
    pub mint: Box<Account<'info, Mint>>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = payer,
    )]
    pub destination: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = payer,
        token::mint = mint,
        token::authority = token_vault, //the PDA address is both the vault account and the authority (and event the mint authority)
        seeds = [ mint.key().as_ref() ],
        bump,
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: This is vault account.
    #[account(
        init_if_needed,
        payer = payer,
        seeds = [ SOL_VAULT_SEED.as_bytes(), mint.key().as_ref() ],
        bump,
        space = 8 + 8
    )]
    pub escrow_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [GLOBAL_INFO_SEED.as_bytes()],
        bump
    )]
    pub global_info: Box<Account<'info, GlobalInfo>>,
    #[account(
        init_if_needed,
        payer = payer,
        seeds = [TOKEN_POOL_SEED.as_bytes(), mint.key().as_ref()],
        bump,
        space = TOKEN_POOL_SIZE
    )]
    pub token_pools: Box<Account<'info, TokenPools>>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metaplex>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct BuyToken<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = token_vault, //the PDA address is both the vault account and the authority (and event the mint authority)
        seeds = [ mint.key().as_ref() ],
        bump,
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: This is vault account.
    #[account(
        mut,
        seeds = [ SOL_VAULT_SEED.as_bytes(), mint.key().as_ref() ],
        bump,
    )]
    pub escrow_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [GLOBAL_INFO_SEED.as_bytes()],
        bump
    )]
    pub global_info: Box<Account<'info, GlobalInfo>>,
    #[account(
        mut,
        seeds = [TOKEN_POOL_SEED.as_bytes(), mint.key().as_ref()],
        bump,
    )]
    pub token_pools: Box<Account<'info, TokenPools>>,
    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = mint,
        associated_token::authority = buyer,
    )]
    pub buyer_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct SellToken<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = token_vault,
        seeds = [ mint.key().as_ref() ],
        bump,
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: This is vault account.
    #[account(
        mut,
        seeds = [ SOL_VAULT_SEED.as_bytes(), mint.key().as_ref() ],
        bump,
    )]
    pub escrow_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [GLOBAL_INFO_SEED.as_bytes()],
        bump
    )]
    pub global_info: Box<Account<'info, GlobalInfo>>,
    #[account(
        mut,
        seeds = [TOKEN_POOL_SEED.as_bytes(), mint.key().as_ref()],
        bump,
    )]
    pub token_pools: Box<Account<'info, TokenPools>>,
    #[account(mut)]
    pub seller_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawBalance<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    /// CHECK: This is vault account.
    #[account(
        mut,
        seeds = [ SOL_VAULT_SEED.as_bytes(), mint.key().as_ref() ],
        bump,
    )]
    pub escrow_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [TOKEN_POOL_SEED.as_bytes(), mint.key().as_ref()],
        bump,
    )]
    pub token_pools: Account<'info, TokenPools>,
    #[account(
        mut,
        seeds = [GLOBAL_INFO_SEED.as_bytes()],
        bump
    )]
    pub global_info: Account<'info, GlobalInfo>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CommonCtx<'info> {
    #[account(
        mut,
        seeds = [GLOBAL_INFO_SEED.as_bytes()],
        bump
    )]
    pub global_info: Account<'info, GlobalInfo>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    #[account()]
    pub mint: Box<Account<'info, Mint>>,
    /// CHECK: Safe. The user coin token
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = payer,
    )]
    pub user_token_coin: Box<Account<'info, TokenAccount>>,
    /// CHECK: Safe. The user pc token
    #[account(mut)]
    pub user_token_pc: UncheckedAccount<'info>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = token_vault,
        seeds = [ mint.key().as_ref() ],
        bump,
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: This is vault account.
    #[account(
        mut,
        seeds = [ SOL_VAULT_SEED.as_bytes(), mint.key().as_ref() ],
        bump,
    )]
    pub escrow_account: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut)]
    pub global_info: Box<Account<'info, GlobalInfo>>,
    #[account(mut)]
    pub token_pools: Box<Account<'info, TokenPools>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

// 5. Define the init token params
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct InitTokenParams {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub decimals: u8,
}

#[account]
pub struct GlobalInfo {
    pub fee_percent: u32,
    pub target_market_cap: u64,
    pub target_lp_amount: u64,
    pub total_supply: u64,
    pub initial_amount: u64,
    pub token_count: u32,
    pub liquidity_added: bool,
    pub owner: Pubkey,
}

#[account]
pub struct TokenPools {
    pub sol_reserve: u64,
    pub token_reserve: u64,
    pub launched: u8, // 0 -> false, 1 -> true
}

#[error_code]
pub enum CustomError {
    #[msg("Initial Amount should not be bigger than 1 ether.")]
    InvalidInitialValue,
    #[msg("Not enough Sol in the pool.")]
    InvalidSolAmount,
    #[msg("Not enough tokens in the pool.")]
    InvalidTokenAmount,
    #[msg("You are not a owner.")]
    NotOwner,
}
