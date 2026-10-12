import {
  AccountRole,
  address,
  appendTransactionMessageInstructions,
  compileTransaction,
  createSolanaRpc,
  createTransactionMessage,
  getAddressDecoder,
  getAddressEncoder,
  getBase58Decoder,
  getBase64Encoder,
  getProgramDerivedAddress,
  getTransactionEncoder,
  getU64Encoder,
  getUtf8Encoder,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
  type Address,
  type Instruction,
} from '@solana/kit';
import deployment from './deployment.json';

export const CLUSTER = deployment.cluster;
export const PROGRAM_ID = address(
  deployment.programs.find((p) => p.lib === 'acorn_jar')!.programId,
);
export const DEFAULT_MINT = address(deployment.token.mint);
export const RPC_URL = 'https://api.mainnet-beta.solana.com';
export const CHAIN = 'solana:mainnet' as const;

export const SYSTEM_PROGRAM = address('11111111111111111111111111111111');
export const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
export const TOKEN_2022_PROGRAM = address('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb');
export const ATA_PROGRAM = address('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

export const MAX_TIP_TOKENS = 1_000_000n;

const IX_CREATE_JAR = 0;
const IX_TIP = 1;
const IX_WITHDRAW = 2;

export const JAR_LEN = 139;
const JAR_DISCRIMINATOR = 1;

export const rpc = createSolanaRpc(RPC_URL);

const addrEnc = getAddressEncoder();
const addrDec = getAddressDecoder();
const utf8 = getUtf8Encoder();

const JAR_ERRORS: Record<number, string> = {
  0: 'A tip of 0 is not allowed.',
  1: 'That tip is above the maximum of 1,000,000 tokens.',
  2: 'Only the jar owner can withdraw.',
  3: 'This token mint is not supported (unsafe Token-2022 extension).',
  4: 'The token account holds a different mint than this jar.',
  5: 'Wrong vault for this jar.',
  6: 'Wrong token program for this jar.',
  7: 'The vault is empty - nothing to withdraw.',
  8: 'This is not a valid jar account.',
  9: 'Invalid withdraw destination.',
};

export type Jar = {
  address: Address;
  vault: Address;
  owner: Address;
  mint: Address;
  tokenProgram: Address;
  tipCount: bigint;
  balance: bigint;
  decimals: number;
};

export async function findJarAddress(owner: Address, mint: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [utf8.encode('jar'), addrEnc.encode(owner), addrEnc.encode(mint)],
  });
  return pda;
}

export async function findVaultAddress(jar: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [utf8.encode('vault'), addrEnc.encode(jar)],
  });
  return pda;
}

export async function findAta(owner: Address, mint: Address, tokenProgram: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: ATA_PROGRAM,
    seeds: [addrEnc.encode(owner), addrEnc.encode(tokenProgram), addrEnc.encode(mint)],
  });
  return pda;
}

type RawAccount = { data: Uint8Array; owner: Address };

async function fetchRaw(addr: Address): Promise<RawAccount | null> {
  const res = await rpc.getAccountInfo(addr, { encoding: 'base64', commitment: 'confirmed' }).send();
  if (!res.value) return null;
  const [b64] = res.value.data;
  return { data: new Uint8Array(getBase64Encoder().encode(b64)), owner: res.value.owner };
}

const u64At = (d: Uint8Array, off: number) =>
  new DataView(d.buffer, d.byteOffset, d.byteLength).getBigUint64(off, true);

export type MintInfo = { tokenProgram: Address; decimals: number };

export async function fetchMint(mint: Address): Promise<MintInfo> {
  const acc = await fetchRaw(mint);
  if (!acc) throw new Error('Token mint not found on mainnet.');
  if (acc.owner !== TOKEN_PROGRAM && acc.owner !== TOKEN_2022_PROGRAM) {
    throw new Error('That address is not a token mint.');
  }
  if (acc.data.length < 82 || acc.data[45] !== 1) throw new Error('That address is not an initialized token mint.');
  return { tokenProgram: acc.owner, decimals: acc.data[44] };
}

export async function fetchJar(owner: Address, mint: Address): Promise<Jar | null> {
  const jarAddress = await findJarAddress(owner, mint);
  const acc = await fetchRaw(jarAddress);
  if (!acc) return null;
  const d = acc.data;
  if (acc.owner !== PROGRAM_ID || d.length !== JAR_LEN || d[0] !== JAR_DISCRIMINATOR) return null;
  const key = (off: number) => addrDec.decode(d.subarray(off, off + 32));
  const vault = key(67);
  const tokenProgram = key(99);
  const [vaultAcc, mintInfo] = await Promise.all([fetchRaw(vault), fetchMint(mint)]);
  return {
    address: jarAddress,
    owner: key(3),
    mint: key(35),
    vault,
    tokenProgram,
    tipCount: u64At(d, 131),
    balance: vaultAcc && vaultAcc.data.length >= 72 ? u64At(vaultAcc.data, 64) : 0n,
    decimals: mintInfo.decimals,
  };
}

export async function fetchTokenBalance(owner: Address, mint: Address, tokenProgram: Address): Promise<bigint> {
  const ata = await findAta(owner, mint, tokenProgram);
  const acc = await fetchRaw(ata);
  return acc && acc.data.length >= 72 ? u64At(acc.data, 64) : 0n;
}

export async function accountExists(addr: Address): Promise<boolean> {
  return (await fetchRaw(addr)) !== null;
}

// ---- instruction builders ----

export async function createJarInstruction(owner: Address, mint: Address, tokenProgram: Address): Promise<Instruction> {
  const jar = await findJarAddress(owner, mint);
  const vault = await findVaultAddress(jar);
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: owner, role: AccountRole.WRITABLE_SIGNER },
      { address: jar, role: AccountRole.WRITABLE },
      { address: vault, role: AccountRole.WRITABLE },
      { address: mint, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
      { address: tokenProgram, role: AccountRole.READONLY },
    ],
    data: new Uint8Array([IX_CREATE_JAR]),
  };
}

export async function tipInstruction(sender: Address, jar: Jar, amount: bigint): Promise<Instruction> {
  const senderToken = await findAta(sender, jar.mint, jar.tokenProgram);
  const data = new Uint8Array(9);
  data[0] = IX_TIP;
  data.set(getU64Encoder().encode(amount), 1);
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: sender, role: AccountRole.READONLY_SIGNER },
      { address: senderToken, role: AccountRole.WRITABLE },
      { address: jar.vault, role: AccountRole.WRITABLE },
      { address: jar.address, role: AccountRole.WRITABLE },
      { address: jar.mint, role: AccountRole.READONLY },
      { address: jar.tokenProgram, role: AccountRole.READONLY },
    ],
    data,
  };
}

export function createAtaIdempotentInstruction(payer: Address, ata: Address, owner: Address, mint: Address, tokenProgram: Address): Instruction {
  return {
    programAddress: ATA_PROGRAM,
    accounts: [
      { address: payer, role: AccountRole.WRITABLE_SIGNER },
      { address: ata, role: AccountRole.WRITABLE },
      { address: owner, role: AccountRole.READONLY },
      { address: mint, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
      { address: tokenProgram, role: AccountRole.READONLY },
    ],
    data: new Uint8Array([1]),
  };
}

export async function withdrawInstructions(owner: Address, jar: Jar): Promise<Instruction[]> {
  const destination = await findAta(owner, jar.mint, jar.tokenProgram);
  return [
    createAtaIdempotentInstruction(owner, destination, owner, jar.mint, jar.tokenProgram),
    {
      programAddress: PROGRAM_ID,
      accounts: [
        { address: owner, role: AccountRole.READONLY_SIGNER },
        { address: jar.address, role: AccountRole.READONLY },
        { address: jar.vault, role: AccountRole.WRITABLE },
        { address: destination, role: AccountRole.WRITABLE },
        { address: jar.mint, role: AccountRole.READONLY },
        { address: jar.tokenProgram, role: AccountRole.READONLY },
      ],
      data: new Uint8Array([IX_WITHDRAW]),
    },
  ];
}

// ---- amounts ----

export function formatAmount(raw: bigint, decimals: number): string {
  const base = 10n ** BigInt(decimals);
  const whole = raw / base;
  const frac = (raw % base).toString().padStart(decimals, '0').replace(/0+$/, '');
  const w = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  return frac ? `${w}.${frac}` : w;
}

export function parseAmount(text: string, decimals: number): bigint | null {
  const t = text.trim();
  if (!/^\d*\.?\d*$/.test(t) || t === '' || t === '.') return null;
  const [whole, frac = ''] = t.split('.');
  if (frac.length > decimals) return null;
  return BigInt(whole || '0') * 10n ** BigInt(decimals) + BigInt(frac.padEnd(decimals, '0') || '0');
}

export const maxTipRaw = (decimals: number) => MAX_TIP_TOKENS * 10n ** BigInt(decimals);

// ---- transactions ----

function friendlyError(err: unknown): string | null {
  const ie = (err as { InstructionError?: [number, unknown] } | null)?.InstructionError;
  if (!ie) return typeof err === 'string' ? `Transaction failed: ${err}` : null;
  const detail = ie[1];
  if (typeof detail === 'object' && detail && 'Custom' in detail) {
    const code = Number((detail as { Custom: number }).Custom);
    return JAR_ERRORS[code] ?? `Program error ${code}.`;
  }
  if (detail === 'InsufficientFunds') return 'Insufficient token balance or SOL for this transaction.';
  return `Transaction would fail: ${typeof detail === 'string' ? detail : JSON.stringify(detail)}`;
}

export async function buildTransaction(payer: Address, instructions: Instruction[]) {
  const { value: blockhash } = await rpc.getLatestBlockhash({ commitment: 'confirmed' }).send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayer(payer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
    (m) => appendTransactionMessageInstructions(instructions, m),
  );
  const tx = compileTransaction(message);
  return { bytes: new Uint8Array(getTransactionEncoder().encode(tx)), lastValidBlockHeight: blockhash.lastValidBlockHeight };
}

/** Simulates first so the user sees a readable program error instead of a wallet failure. */
export async function simulate(bytes: Uint8Array): Promise<void> {
  const b64 = btoa(String.fromCharCode(...bytes));
  const res = await rpc
    .simulateTransaction(b64 as never, { encoding: 'base64', sigVerify: false, replaceRecentBlockhash: true, commitment: 'confirmed' })
    .send();
  const msg = friendlyError(res.value.err);
  if (msg) throw new Error(msg);
}

export async function waitForConfirmation(signatureBytes: Uint8Array): Promise<string> {
  const signature = getBase58Decoder().decode(signatureBytes);
  for (let i = 0; i < 60; i++) {
    const { value } = await rpc.getSignatureStatuses([signature as never], { searchTransactionHistory: true }).send();
    const st = value[0];
    if (st) {
      if (st.err) throw new Error(friendlyError(st.err) ?? 'Transaction failed.');
      if (st.confirmationStatus === 'confirmed' || st.confirmationStatus === 'finalized') return signature;
    }
    await new Promise((r) => setTimeout(r, 1500));
  }
  throw new Error(`Not confirmed yet - check signature ${signature} in an explorer.`);
}
