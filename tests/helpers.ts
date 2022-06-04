import { BN, Provider, utils } from "@project-serum/anchor";
// eslint-disable-next-line node/no-extraneous-import
import { PublicKey, LAMPORTS_PER_SOL, Keypair } from "@solana/web3.js";
import * as spl from "@solana/spl-token";
import { Account } from "@solana/spl-token";

const NFT_COLLATERIZED_LOANS_SEED: string = "config";
const NFT_COLLATERIZED_LOANS_ST_VAULT_SEED: string = "st_vault";
const NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED: string = "nft_vault";

// airdrop SOL
export const airdropSOL = async (
  provider: Provider,
  to: PublicKey,
  solAmount: number
): Promise<void> => {
  const signature = await provider.connection.requestAirdrop(
    to,
    solAmount * LAMPORTS_PER_SOL
  );
  await provider.connection.confirmTransaction(signature);
};

// create Token
export const createTokenMint = async (
  provider: Provider,
  payer: Keypair,
  mintAuthority: PublicKey,
  freezeAuthority: PublicKey | null,
  decimals: number
): Promise<PublicKey> => {
  return await spl.createMint(
    provider.connection,
    payer,
    mintAuthority,
    freezeAuthority,
    decimals
  );
};

// mint Token to account
export const mintTokenTo = async (
  provider: Provider,
  payer: Keypair,
  mint: PublicKey,
  to: PublicKey,
  authority: PublicKey,
  amount: number
): Promise<Account> => {
  const tokenAccount = await spl.getOrCreateAssociatedTokenAccount(
    provider.connection,
    payer,
    mint,
    to
  );
  await spl.mintTo(
    provider.connection,
    payer,
    mint,
    tokenAccount.address,
    authority,
    amount
  );
  return tokenAccount;
};

// create NFT to account
export const createNFT = async (
  provider: Provider,
  payer: Keypair,
  to: PublicKey
): Promise<[PublicKey, Account]> => {
  // create nft mint
  const nftMint = await spl.createMint(
    provider.connection,
    payer,
    payer.publicKey,
    null,
    0
  );
  // create user`s nft account
  const userNFTAccount = await spl.getOrCreateAssociatedTokenAccount(
    provider.connection,
    payer,
    nftMint,
    to
  );
  // mint nft to user
  await spl.mintTo(
    provider.connection,
    payer,
    nftMint,
    userNFTAccount.address,
    payer.publicKey,
    1
  );

  return [nftMint, userNFTAccount];
};

// stable coin account pda
export const deriveSCAccountPDA = async (
  scMint: PublicKey,
  programId: PublicKey
): Promise<[PublicKey, number]> => {
  return await PublicKey.findProgramAddress(
    [
      scMint.toBuffer(),
      Buffer.from(
        utils.bytes.utf8.encode(NFT_COLLATERIZED_LOANS_ST_VAULT_SEED)
      ),
    ],
    programId
  );
};

// configuration account pda
export const deriveConfigurationAccountPDA = async (
  scMint: PublicKey,
  programId: PublicKey
): Promise<[PublicKey, number]> => {
  return await PublicKey.findProgramAddress(
    [
      scMint.toBuffer(),
      Buffer.from(utils.bytes.utf8.encode(NFT_COLLATERIZED_LOANS_SEED)),
    ],
    programId
  );
};

// NFT account pda
export const deriveNFTAccountPDA = async (
  nftMint: PublicKey,
  programId: PublicKey
): Promise<[PublicKey, number]> => {
  return await PublicKey.findProgramAddress(
    [
      nftMint.toBuffer(),
      Buffer.from(
        utils.bytes.utf8.encode(NFT_COLLATERIZED_LOANS_NFT_VAULT_SEED)
      ),
    ],
    programId
  );
};

// order account pda
export const deriveOrderAccountPDA = async (
  configuration: PublicKey,
  orderId: BN,
  programId: PublicKey
): Promise<[PublicKey, number]> => {
  return await PublicKey.findProgramAddress(
    [
      Buffer.from(utils.bytes.utf8.encode(orderId.toString())),
      configuration.toBuffer(),
    ],
    programId
  );
};

export const sleep = (ms) => {
  return new Promise((resolve) => setTimeout(resolve, ms));
};
