import { expect } from 'chai';
import { ethers } from 'hardhat';

describe('Security Event Logging', () => {
  async function setup() {
    const [owner, user] = await ethers.getSigners();

    const KYCManager = await ethers.getContractFactory('KYCManager');
    const kyc = await KYCManager.deploy(owner.address);

    const RoleManager = await ethers.getContractFactory('RoleManager');
    const roles = await RoleManager.deploy(owner.address);

    const GovernanceManager = await ethers.getContractFactory(
      'GovernanceManager',
    );
    const governance = await GovernanceManager.deploy();

    return {
      owner,
      user,
      kyc,
      roles,
      governance,
    };
  }

  it('emits KYCUpdated event', async () => {
    const { kyc, owner, user } = await setup();

    await expect(kyc.connect(owner).updateKYC(user.address, true))
      .to.emit(kyc, 'KYCUpdated')
      .withArgs(owner.address, user.address, true, anyValue);
  });

  it('emits RoleUpdated event on role grant', async () => {
    const { roles, owner, user } = await setup();

    const role = ethers.keccak256(ethers.toUtf8Bytes('ADMIN_ROLE'));

    await expect(roles.connect(owner).grantAdminRole(user.address))
      .to.emit(roles, 'RoleUpdated')
      .withArgs(owner.address, user.address, role, true, anyValue);
  });

  it('emits RoleUpdated event on role revoke', async () => {
    const { roles, owner, user } = await setup();

    const role = ethers.keccak256(ethers.toUtf8Bytes('ADMIN_ROLE'));

    await roles.connect(owner).grantAdminRole(user.address);

    await expect(roles.connect(owner).revokeAdminRole(user.address))
      .to.emit(roles, 'RoleUpdated')
      .withArgs(owner.address, user.address, role, false, anyValue);
});