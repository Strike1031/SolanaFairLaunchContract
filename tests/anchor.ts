import BN from "bn.js";
import assert from "assert";
import * as web3 from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import type { TokenMinter } from "../target/types/token_minter";
describe("Test Minter", () => {
  // Configure the client to use the local cluster
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.TokenMinter as anchor.Program<TokenMinter>;
  
  // Metaplex Constants
  const METADATA_SEED = "metadata";
  const TOKEN_METADATA_PROGRAM_ID = new web3.PublicKey(
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
  );

  // Constants from our program
  const MINT_SEED = "mint";
  const GLOBAL_INFO_SEED = "global_info";
  const TOKEN_POOL_SEED = "token_pool";
  const SOL_VAULT_SEED = "sol_escrow_seed";

  const tokenName = "great123"
  // Data for our tests
  const payer = program.provider.publicKey;
  const metadata = {
    name: tokenName,
    symbol: "TEST",
    uri: "https://5vfxc4tr6xoy23qefqbj4qx2adzkzapneebanhcalf7myvn5gzja.arweave.net/7UtxcnH13Y1uBCwCnkL6APKsge0hAgacQFl-zFW9NlI",
    decimals: 9,
  };
  const mintAmount = 0.1;
  const [mint] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from(MINT_SEED), Buffer.from(tokenName)],
    program.programId
  );

  const [tokenVault] = web3.PublicKey.findProgramAddressSync(
    [mint.toBuffer()],
    program.programId
  );

  const [escrowAccount] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from(SOL_VAULT_SEED), mint.toBuffer()],
    program.programId
  );

  const [globalInfo] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from(GLOBAL_INFO_SEED)],
    program.programId
  );

  const [tokenPools] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from(TOKEN_POOL_SEED), mint.toBuffer()],
    program.programId
  );

  console.log("mint", mint.toBase58());
  console.log("tokenVault", tokenVault.toBase58());
  console.log("escrowAccount", escrowAccount.toBase58());
  console.log("globalInfo", globalInfo.toBase58());
  console.log("tokenPools", tokenPools.toBase58());
  const [metadataAddress] = web3.PublicKey.findProgramAddressSync(
    [
      Buffer.from(METADATA_SEED),
      TOKEN_METADATA_PROGRAM_ID.toBuffer(),
      mint.toBuffer(),
    ],
    TOKEN_METADATA_PROGRAM_ID
  );

    // Test init token
  it("create token", async () => {
    const info = await program.provider.connection.getAccountInfo(mint);
    if (info) {
      console.log("Already minted!!!")
      return; // Do not attempt to initialize if already initialized
    }
    console.log("  Mint not found. Attempting to initialize.");

    const destination = await anchor.utils.token.associatedAddress({
      mint: mint,
      owner: payer,
    });

    const context = {
      metadata: metadataAddress,
      mint,
      destination,
      tokenVault,
      escrowAccount,
      globalInfo,
      tokenPools,
      payer,
      rent: web3.SYSVAR_RENT_PUBKEY,
      systemProgram: web3.SystemProgram.programId,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
      associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
    };

    const txHash = await program.methods
      .createToken(metadata, new BN(mintAmount * 10 ** metadata.decimals))
      .accounts(context)
      .rpc()
      .catch(e => console.log(e));

    await program.provider.connection.confirmTransaction(txHash, "finalized");
    console.log(`  https://explorer.solana.com/tx/${txHash}?cluster=devnet`);
    const newInfo = await program.provider.connection.getAccountInfo(mint);
    assert(newInfo, "  Mint should be initialized.");
  });

  it("buy token", async () => {
     const destination = await anchor.utils.token.associatedAddress({
      mint: mint,
      owner: payer,
    });

    const context = {
      buyer: payer,
      mint,
      tokenVault,
      escrowAccount,
      globalInfo,
      tokenPools, 
      buyerTokenAccount: destination,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
      associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
    };
    console.log("destination", destination.toBase58());
    const txHash = await program.methods
      .buyToken(new BN(0.1 * 10 ** metadata.decimals))
      .accounts(context)
      .rpc()
      .catch(e => console.log(e));

    await program.provider.connection.confirmTransaction(txHash, "finalized");
    console.log(`  https://explorer.solana.com/tx/${txHash}?cluster=devnet`);
  });

  it("sell token", async () => {
     const destination = await anchor.utils.token.associatedAddress({
      mint: mint,
      owner: payer,
    });

    const context = {
      seller: payer,
      mint,
      tokenVault,
      escrowAccount,
      globalInfo,
      tokenPools, 
      sellerTokenAccount: destination,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
    };
    console.log("destination", destination.toBase58());
    const txHash = await program.methods
      .sellToken(new BN(1000000 * 10 ** metadata.decimals))
      .accounts(context)
      .rpc()
      .catch(e => console.log(e));

    await program.provider.connection.confirmTransaction(txHash, "finalized");
    console.log(`  https://explorer.solana.com/tx/${txHash}?cluster=devnet`);
  });

  it("withdraw", async () => {
   
    const context = {
      mint,
      escrowAccount,
      tokenPools, 
      globalInfo,
      admin: payer,
      systemProgram: web3.SystemProgram.programId,
    };
    const txHash = await program.methods
      .withdrawBalance(new BN(0.05 * 10 ** metadata.decimals))
      .accounts(context)
      .rpc()
      .catch(e => console.log(e));

    await program.provider.connection.confirmTransaction(txHash, "finalized");
    console.log(`  https://explorer.solana.com/tx/${txHash}?cluster=devnet`);
  });

  it("add liquidity", async () => {
    const userTokenCoin = await anchor.utils.token.associatedAddress({
      mint: mint,
      owner: payer,
    });
    const userTokenPc = await anchor.utils.token.associatedAddress({
      mint: new web3.PublicKey("So11111111111111111111111111111111111111112"),
      owner: payer,
    });
    const context = {
      mint,
      userTokenCoin,
      userTokenPc,
      tokenVault,
      escrowAccount,
      payer,
      globalInfo,
      tokenPools, 
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
      associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
    };
    const txHash = await program.methods
      .addLiquidity(new BN(100000000))
      .accounts(context)
      .rpc()
      .catch(e => console.log(e));

    await program.provider.connection.confirmTransaction(txHash, "finalized");
    console.log(`  https://explorer.solana.com/tx/${txHash}?cluster=devnet`);
  });
});
