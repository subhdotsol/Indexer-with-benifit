import { getCreateAccountInstruction } from "@solana-program/system";
import {
  findAssociatedTokenPda,
  getCreateAssociatedTokenInstruction,
  getInitializeMintInstruction,
  getMintSize,
  getMintToInstruction,
  getTransferCheckedInstruction,
  TOKEN_PROGRAM_ADDRESS,
} from "@solana-program/token";
import {
  airdropFactory,
  appendTransactionMessageInstructions,
  assertIsTransactionMessageWithBlockhashLifetime,
  assertIsTransactionMessageWithDurableNonceLifetime,
  assertIsTransactionMessageWithinSizeLimit,
  assertIsTransactionWithBlockhashLifetime,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  generateKeyPairSigner,
  lamports,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type Instruction,
  type KeyPairSigner,
  type TransactionSigner,
} from "@solana/kit";

const rpc = createSolanaRpc("http://localhost:8899");
const rpcSubscriptions = createSolanaRpcSubscriptions("ws://localhost:8900");

async function sendInstructions({
  payer,
  instructions,
}: {
  payer: KeyPairSigner;
  instructions: Instruction[];
}) {
  const { value: blockhash } = await rpc.getLatestBlockhash().send();

  const txMsg = pipe(
    createTransactionMessage({ version: 0 }),
    (tx) => setTransactionMessageFeePayerSigner(payer, tx),
    (tx) => setTransactionMessageLifetimeUsingBlockhash(blockhash, tx),
    (tx) => appendTransactionMessageInstructions(instructions, tx)
  );

  assertIsTransactionMessageWithinSizeLimit(txMsg);
  assertIsTransactionMessageWithBlockhashLifetime(txMsg);

  const signedTx = await signTransactionMessageWithSigners(txMsg);
  assertIsTransactionWithBlockhashLifetime(signedTx);

  const sendAndConfirmTransactionMethod = sendAndConfirmTransactionFactory({
    rpc,
    rpcSubscriptions,
  });

  await sendAndConfirmTransactionMethod(signedTx, {
    commitment: "confirmed",
  });
}

async function createMint(payer: KeyPairSigner): Promise<Address> {
  const mintKeypair = await generateKeyPairSigner();

  const MINT_SIZE = BigInt(getMintSize());

  const rent = await rpc.getMinimumBalanceForRentExemption(MINT_SIZE).send();

  const createAccountIx = getCreateAccountInstruction({
    lamports: rent,
    payer: payer,
    space: MINT_SIZE,
    newAccount: mintKeypair,
    programAddress: TOKEN_PROGRAM_ADDRESS,
  });

  const initializeMintIX = getInitializeMintInstruction({
    mint: mintKeypair.address,
    decimals: 9,
    mintAuthority: payer.address,
  });

  const IX: Instruction[] = [createAccountIx, initializeMintIX];

  await sendInstructions({ payer, instructions: IX });

  return mintKeypair.address;
}

async function createATA(
  payer: KeyPairSigner,
  mintAddress: Address
): Promise<Address> {
  // Find Program Address , check if alreday exists otherwise create one
  const [ataAddress] = await findAssociatedTokenPda({
    mint: mintAddress,
    owner: payer.address,
    tokenProgram: TOKEN_PROGRAM_ADDRESS,
  });

  const createATAIX = getCreateAssociatedTokenInstruction({
    ata: ataAddress,
    mint: mintAddress,
    owner: payer.address,
    payer: payer,
  });

  await sendInstructions({ payer, instructions: [createATAIX] });

  return ataAddress;
}

async function transferToken({
  userA_ata,
  userB_ata,
  userA,
  mintAddress,
  counter,
}: {
  userA_ata: Address;
  userB_ata: Address;
  userA: KeyPairSigner;
  mintAddress: Address;
  counter: number;
}) {
  try {
    const transferIX = getTransferCheckedInstruction({
      amount: 100 + counter,
      authority: userA.address,
      decimals: 9,
      destination: userB_ata,
      source: userA_ata,
      mint: mintAddress,
    });

    console.log("TOKEN ADDRESS", TOKEN_PROGRAM_ADDRESS);

    await sendInstructions({ payer: userA, instructions: [transferIX] });
    console.log("Transferred Token Successful");
  } catch (e) {
    console.error("Error", e);
  }
}

async function mintTokens(
  ataAddress: Address,
  mintAuthority: KeyPairSigner,
  mintAddress: Address
) {
  // first have to verify that ATA exists or not
  const mintIX = getMintToInstruction({
    amount: 1_000_000_000,
    mint: mintAddress,
    mintAuthority: mintAuthority,
    token: ataAddress,
  });

  await sendInstructions({ payer: mintAuthority, instructions: [mintIX] });
}

async function main() {
  const userA = await generateKeyPairSigner();
  const userB = await generateKeyPairSigner();

  const LAMPORTS = 1_000_000_000n;

  const airDrop = airdropFactory({ rpc, rpcSubscriptions });

  await airDrop({
    recipientAddress: userA.address,
    commitment: "confirmed",
    lamports: lamports(LAMPORTS),
  });

  await airDrop({
    recipientAddress: userB.address,
    commitment: "confirmed",
    lamports: lamports(LAMPORTS),
  });

  const mintAddress = await createMint(userA);

  console.log("Mint Address", mintAddress);

  const userA_ata = await createATA(userA, mintAddress);
  const userB_ata = await createATA(userB, mintAddress);

  await mintTokens(userA_ata, userA, mintAddress);

  let counter = 0;

  setInterval(async () => {
    counter += 1;
    await transferToken({
      mintAddress,
      userA,
      userA_ata,
      userB_ata,
      counter,
    });
  }, 1000);
}

main();
