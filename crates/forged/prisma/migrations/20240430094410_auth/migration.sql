/*
  Warnings:

  - Added the required column `authconf` to the `Domain` table without a default value. This is not possible if the table is not empty.
  - Added the required column `private_key` to the `Domain` table without a default value. This is not possible if the table is not empty.
  - Added the required column `public_key` to the `Domain` table without a default value. This is not possible if the table is not empty.

*/
-- AlterEnum
-- This migration adds more than one value to an enum.
-- With PostgreSQL versions 11 and earlier, this is not possible
-- in a single migration. This can be worked around by creating
-- multiple migrations, each migration adding only one value to
-- the enum.


ALTER TYPE "KeyType" ADD VALUE 'Rsa';
ALTER TYPE "KeyType" ADD VALUE 'ECDSA';

-- AlterTable
ALTER TABLE "Actor" ADD COLUMN     "remote_handles" TEXT[];

-- AlterTable
ALTER TABLE "Domain" ADD COLUMN     "authconf" JSONB NOT NULL,
ADD COLUMN     "private_key" TEXT NOT NULL,
ADD COLUMN     "public_key" TEXT NOT NULL;

-- AlterTable
ALTER TABLE "Key" ALTER COLUMN "private_key" DROP NOT NULL;
