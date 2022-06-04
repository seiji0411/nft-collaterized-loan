import * as anchor from "@project-serum/anchor";
import { BN, Program } from "@project-serum/anchor";
import { NftLoans } from "../target/types/nft_loans";
import {
  Account,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import {
  airdropSOL,
  createNFT,
  createTokenMint,
  deriveConfigurationAccountPDA,
  deriveNFTAccountPDA,
  deriveOrderAccountPDA,
  deriveSCAccountPDA,
  mintTokenTo,
  sleep,
} from "./helpers";
import { LAMPORTS_PER_SOL, PublicKey, Keypair } from "@solana/web3.js";
import { expect } from "chai";

const FEE_PT = 10;

describe("nft-loans", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  const SYSTEM_PROGRAM_ID = anchor.web3.SystemProgram.programId;
  const SYSVAR_RENT_PUBKEY = anchor.web3.SYSVAR_RENT_PUBKEY;

  const program = anchor.workspace.NftLoans as Program<NftLoans>;

  // owner
  const owner = Keypair.generate();

  // users
  const alice = Keypair.generate();
  const bob = Keypair.generate();

  // stable coin
  let stableCoinMint: PublicKey;

  // users
  let aliceSCAccount: Account;
  let bobSCAccount: Account;

  // nft
  let nftMint: PublicKey;
  let aliceNftAccount: Account;

  before(async () => {
    // airdrop
    await airdropSOL(program.provider, owner.publicKey, 20);
    await airdropSOL(program.provider, alice.publicKey, 20);
    await airdropSOL(program.provider, bob.publicKey, 20);

    // check balance
    expect(await program.provider.connection.getBalance(owner.publicKey)).to.eq(
      20 * LAMPORTS_PER_SOL
    );

    // create stable coin
    stableCoinMint = await createTokenMint(
      program.provider,
      owner,
      owner.publicKey,
      null,
      0
    );

    // mint stable coin to users
    aliceSCAccount = await mintTokenTo(
      program.provider,
      owner,
      stableCoinMint,
      alice.publicKey,
      owner.publicKey,
      1000
    );
    bobSCAccount = await mintTokenTo(
      program.provider,
      owner,
      stableCoinMint,
      bob.publicKey,
      owner.publicKey,
      1000
    );

    // create NFT
    [nftMint, aliceNftAccount] = await createNFT(
      program.provider,
      alice,
      alice.publicKey
    );

    // check supply
    const supply = await program.provider.connection.getTokenSupply(
      stableCoinMint
    );
    expect(supply.value.amount).to.eq("2000");
    expect(supply.value.decimals).to.eq(0);
  });

  it("Is initialized!", async () => {
    // get pda for stable coin account of program
    const [programSCVault] = await deriveSCAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [configurationPubKey] = await deriveConfigurationAccountPDA(
      stableCoinMint,
      program.programId
    );

    await program.methods
      .initialize(FEE_PT)
      .accounts({
        signer: owner.publicKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        configuration: configurationPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([owner])
      .rpc();

    // check configuration
    const configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    expect(configuration.stablecoinMint.toBase58()).to.eq(
      stableCoinMint.toBase58()
    );
    expect(configuration.stablecoinVault.toBase58()).to.eq(
      programSCVault.toBase58()
    );
    expect(configuration.orderId.toNumber()).to.eq(0);
    expect(configuration.totalAdditionalCollateral.toNumber()).to.eq(0);
    expect(configuration.feePt).to.eq(FEE_PT);
  });

  it("Create order!", async () => {
    // pda
    const [programSCVault] = await deriveSCAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [configurationPubKey] = await deriveConfigurationAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [programNFTVault] = await deriveNFTAccountPDA(
      nftMint,
      program.programId
    );

    // order pda
    let configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    const [orderPubKey] = await deriveOrderAccountPDA(
      configurationPubKey,
      configuration.orderId,
      program.programId
    );

    // 3 days
    const period: BN = new BN(3 * 86400);
    const requestAmount: BN = new BN(100);
    const interest: BN = new BN(10);
    const additionalCollateral: BN = new BN(10);

    await program.methods
      .createOrder(requestAmount, interest, period, additionalCollateral)
      .accounts({
        signer: alice.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        userStablecoinVault: aliceSCAccount.address,
        nftMint,
        nftVault: programNFTVault,
        userNftVault: aliceNftAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([alice])
      .rpc();

    // check configuration
    const order = await program.account.order.fetch(orderPubKey);
    expect(order.borrower.toBase58()).to.eq(alice.publicKey.toBase58());
    expect(order.stablecoinVault.toBase58()).to.eq(programSCVault.toBase58());
    expect(order.nftMint.toBase58()).to.eq(nftMint.toBase58());
    expect(order.nftVault.toBase58()).to.eq(programNFTVault.toBase58());
    expect(order.requestAmount.toNumber()).to.eq(requestAmount.toNumber());
    expect(order.interest.toNumber()).to.eq(interest.toNumber());
    expect(order.period.toNumber()).to.eq(period.toNumber());
    expect(order.additionalCollateral.toNumber()).to.eq(
      additionalCollateral.toNumber()
    );
    expect(order.orderStatus).to.eq(true);

    // check configuration
    configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    expect(configuration.orderId.toNumber()).to.eq(1);
  });

  it("Give loan!", async () => {
    // pda
    const [programSCVault] = await deriveSCAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [configurationPubKey] = await deriveConfigurationAccountPDA(
      stableCoinMint,
      program.programId
    );

    // order pda
    const orderId: BN = new BN(0);
    const [orderPubKey] = await deriveOrderAccountPDA(
      configurationPubKey,
      orderId,
      program.programId
    );

    await program.methods
      .giveLoan(orderId)
      .accounts({
        signer: bob.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        lenderStablecoinVault: bobSCAccount.address,
        borrowerStablecoinVault: aliceSCAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([bob])
      .rpc();

    // check order
    const order = await program.account.order.fetch(orderPubKey);
    expect(order.lender.toBase58()).to.eq(bob.publicKey.toBase58());
    expect(order.loanStartTime.toNumber()).to.not.eq(0);
    expect(order.orderStatus).to.eq(false);
  });

  it("Pay back!", async () => {
    // pda
    const [programSCVault] = await deriveSCAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [configurationPubKey] = await deriveConfigurationAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [programNFTVault] = await deriveNFTAccountPDA(
      nftMint,
      program.programId
    );

    // order pda
    const order_id: BN = new BN(0);
    const [orderPubKey] = await deriveOrderAccountPDA(
      configurationPubKey,
      order_id,
      program.programId
    );

    await program.methods
      .payback(order_id)
      .accounts({
        signer: alice.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        lenderStablecoinVault: bobSCAccount.address,
        userStablecoinVault: aliceSCAccount.address,
        nftMint,
        nftVault: programNFTVault,
        userNftVault: aliceNftAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([alice])
      .rpc();

    // check order
    let isExisting = true;
    try {
      await program.account.order.fetch(orderPubKey);
    } catch (e) {
      isExisting = false;
    }
    expect(isExisting).to.eq(false);
  });

  it("Cancel order!", async () => {
    // create NFT
    [nftMint, aliceNftAccount] = await createNFT(
      program.provider,
      alice,
      alice.publicKey
    );

    // pda
    const [programSCVault, programSCNonce] = await deriveSCAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [configurationPubKey] = await deriveConfigurationAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [programNFTVault] = await deriveNFTAccountPDA(
      nftMint,
      program.programId
    );

    // order pda
    let configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    const order_id = configuration.orderId;
    const [orderPubKey] = await deriveOrderAccountPDA(
      configurationPubKey,
      order_id,
      program.programId
    );

    // 3 days
    const period: BN = new BN(3 * 86400);
    const requestAmount: BN = new BN(100);
    const interest: BN = new BN(10);
    const additionalCollateral: BN = new BN(10);

    // Create order
    await program.methods
      .createOrder(requestAmount, interest, period, additionalCollateral)
      .accounts({
        signer: alice.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        userStablecoinVault: aliceSCAccount.address,
        nftMint,
        nftVault: programNFTVault,
        userNftVault: aliceNftAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([alice])
      .rpc();

    // Cancel Order
    await program.methods
      .cancelOrder(order_id)
      .accounts({
        signer: alice.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        userStablecoinVault: aliceSCAccount.address,
        nftMint,
        nftVault: programNFTVault,
        userNftVault: aliceNftAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([alice])
      .rpc();

    // check configuration
    configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    expect(configuration.orderId.toNumber()).to.eq(1 + order_id.toNumber());
    expect(configuration.totalAdditionalCollateral.toNumber()).to.eq(0);

    // check order
    let isExisting = true;
    try {
      await program.account.order.fetch(orderPubKey);
    } catch (e) {
      isExisting = false;
    }
    expect(isExisting).to.eq(false);
  });

  it("Liquidate!", async () => {
    // create NFT
    [nftMint, aliceNftAccount] = await createNFT(
      program.provider,
      alice,
      alice.publicKey
    );

    // pda
    const [programSCVault, programSCNonce] = await deriveSCAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [configurationPubKey] = await deriveConfigurationAccountPDA(
      stableCoinMint,
      program.programId
    );
    const [programNFTVault] = await deriveNFTAccountPDA(
      nftMint,
      program.programId
    );

    // order pda
    let configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    const order_id = configuration.orderId;
    const [orderPubKey] = await deriveOrderAccountPDA(
      configurationPubKey,
      order_id,
      program.programId
    );

    // 3s
    const period: BN = new BN(3);
    const requestAmount: BN = new BN(100);
    const interest: BN = new BN(10);
    const additionalCollateral: BN = new BN(10);

    // Create order
    await program.methods
      .createOrder(requestAmount, interest, period, additionalCollateral)
      .accounts({
        signer: alice.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        userStablecoinVault: aliceSCAccount.address,
        nftMint,
        nftVault: programNFTVault,
        userNftVault: aliceNftAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([alice])
      .rpc();

    // Give loan
    await program.methods
      .giveLoan(order_id)
      .accounts({
        signer: bob.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        lenderStablecoinVault: bobSCAccount.address,
        borrowerStablecoinVault: aliceSCAccount.address,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([bob])
      .rpc();

    // create bob nft token account address
    const bobNftTokenAccountPubKey = await getAssociatedTokenAddress(
      nftMint,
      bob.publicKey
    );
    const instruction = createAssociatedTokenAccountInstruction(
      bob.publicKey,
      bobNftTokenAccountPubKey,
      bob.publicKey,
      nftMint
    );

    await sleep(5000);

    // liquidate
    await program.methods
      .liquidate(order_id)
      .accounts({
        signer: bob.publicKey,
        configuration: configurationPubKey,
        stablecoinMint: stableCoinMint,
        stablecoinVault: programSCVault,
        lenderStablecoinVault: bobSCAccount.address,
        nftMint,
        nftVault: programNFTVault,
        userNftVault: bobNftTokenAccountPubKey,
        order: orderPubKey,
        systemProgram: SYSTEM_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .preInstructions([instruction])
      .signers([bob])
      .rpc();

    // check configuration
    configuration = await program.account.configuration.fetch(
      configurationPubKey
    );
    expect(configuration.orderId.toNumber()).to.eq(1 + order_id.toNumber());
    expect(configuration.totalAdditionalCollateral.toNumber()).to.eq(0);

    // check order
    let isExisting = true;
    try {
      await program.account.order.fetch(orderPubKey);
    } catch (e) {
      isExisting = false;
    }
    expect(isExisting).to.eq(false);
  });
});
