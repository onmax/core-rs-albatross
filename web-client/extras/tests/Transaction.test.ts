import { PrivateKey, PublicKey, KeyPair, Transaction, AccountType, TransactionFormat, Address } from '@nimiq/core';
import { describe, expect, it } from 'vitest';

describe('Transaction', () => {
    it('has the correct types for format and sender/recipient type', () => {
        const sender = PrivateKey.fromHex("41771e617f4a847642f9c0cd0fdb9b22b0c9dfdd2548fe7a6cad0a782e588f63")
        const recipient = Address.fromUserFriendlyAddress("NQ72 9DS3 CQHA G44E ND6G DCQT 561M 3G2A H9FL");

        const transaction = new Transaction(
            PublicKey.derive(sender).toAddress(),
            AccountType.Basic,
            new Uint8Array(0),
            recipient,
            AccountType.Basic,
            new Uint8Array(0),
            100_00000n,
            0n,
            0,
            1,
            6,
        );
        transaction.sign(KeyPair.derive(sender));

        // Verify that the enums are numbers
        expect(AccountType.Basic).toBe(0);
        expect(TransactionFormat.Basic).toBe(0);

        // Now check the transaction fields
        expect(transaction.format).toBe(TransactionFormat.Basic);
        expect(transaction.senderType).toBe(AccountType.Basic);
        expect(transaction.recipientType).toBe(AccountType.Basic);

        // In plain format, the fields should now be strings
        const plain = transaction.toPlain();
        expect(plain.format).toBe('basic');
        expect(plain.senderType).toBe('basic');
        expect(plain.recipientType).toBe('basic');
    });
});
