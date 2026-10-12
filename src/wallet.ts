import { getWallets } from '@wallet-standard/app';
import type { Wallet, WalletAccount } from '@wallet-standard/base';
import { StandardConnect, StandardDisconnect, type StandardConnectFeature, type StandardDisconnectFeature } from '@wallet-standard/features';
import { SolanaSignAndSendTransaction, type SolanaSignAndSendTransactionFeature } from '@solana/wallet-standard-features';
import { address, type Address } from '@solana/kit';
import { CHAIN } from './client';

export type Connected = { wallet: Wallet; account: WalletAccount; address: Address };

export function listWallets(): Wallet[] {
  return getWallets()
    .get()
    .filter(
      (w) =>
        StandardConnect in w.features &&
        SolanaSignAndSendTransaction in w.features &&
        w.chains.includes(CHAIN),
    );
}

export function onWalletsChanged(cb: () => void): () => void {
  const w = getWallets();
  const offA = w.on('register', cb);
  const offB = w.on('unregister', cb);
  return () => {
    offA();
    offB();
  };
}

export async function connectWallet(wallet: Wallet): Promise<Connected> {
  const feature = (wallet.features as unknown as StandardConnectFeature)[StandardConnect];
  const { accounts } = await feature.connect();
  const account = accounts.find((a) => a.chains.includes(CHAIN)) ?? accounts[0];
  if (!account) throw new Error('The wallet did not return an account.');
  return { wallet, account, address: address(account.address) };
}

export async function disconnectWallet(wallet: Wallet): Promise<void> {
  const f = (wallet.features as unknown as Partial<StandardDisconnectFeature>)[StandardDisconnect];
  await f?.disconnect();
}

export async function signAndSend(c: Connected, transaction: Uint8Array): Promise<Uint8Array> {
  const f = (c.wallet.features as unknown as SolanaSignAndSendTransactionFeature)[SolanaSignAndSendTransaction];
  const [out] = await f.signAndSendTransaction({ account: c.account, chain: CHAIN, transaction });
  return out.signature;
}
