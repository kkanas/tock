//! Power Reset Clock Interrupt controller driver.

use kernel::common::registers::{register_bitfields, ReadWrite};
use kernel::common::StaticRef;

#[repr(C)]
pub struct PrciRegisters {
    /// Clock Configuration Register
    hfrosccfg: ReadWrite<u32, hfrosccfg::Register>,
    /// Clock Configuration Register
    hfxosccfg: ReadWrite<u32, hfxosccfg::Register>,
    /// PLL Configuration Register
    pllcfg: ReadWrite<u32, pllcfg::Register>,
    /// PLL Divider Register
    plloutdiv: ReadWrite<u32, plloutdiv::Register>,
    /// Clock Configuration Register
    coreclkcfg: ReadWrite<u32>,
}

register_bitfields![u32,
    hfrosccfg [
        ready OFFSET(31) NUMBITS(1) [],
        enable OFFSET(30) NUMBITS(1) [],
        trim OFFSET(16) NUMBITS(5) [],
        div OFFSET(0) NUMBITS(6) []
    ],
    hfxosccfg [
        ready OFFSET(31) NUMBITS(1) [],
        enable OFFSET(30) NUMBITS(1) []
    ],
    pllcfg [
        lock OFFSET(31) NUMBITS(1) [],
        bypass OFFSET(18) NUMBITS(1) [],
        refsel OFFSET(17) NUMBITS(1) [],
        sel OFFSET(16) NUMBITS(1) [],
        pllq OFFSET(10) NUMBITS(2) [],
        pllf OFFSET(4) NUMBITS(6) [],
        pllr OFFSET(0) NUMBITS(2) [
            R1 = 0,
            R2 = 1,
            R3 = 2,
            R4 = 3
        ]
    ],
    plloutdiv [
        divby1 OFFSET(8) NUMBITS(1) [],
        div OFFSET(0) NUMBITS(6) []
    ]
];

pub enum ClockFrequency {
    Freq18Mhz,
    Freq384Mhz,
}

pub struct Prci {
    registers: StaticRef<PrciRegisters>,
}

impl Prci {
    pub const fn new(base: StaticRef<PrciRegisters>) -> Prci {
        Prci { registers: base }
    }

    pub fn set_clock_frequency(&self, frequency: ClockFrequency) {
        let regs = self.registers;

        // debug!("reg {:#x}", regs.hfrosccfg.get());

        // Assume a 72 MHz clock, then `div` is (72/frequency) - 1.
        match frequency {
            ClockFrequency::Freq18Mhz => {
                // 4, // this seems wrong, but it works??
                // regs.hfrosccfg.modify(hfrosccfg::enable::SET + hfrosccfg::div.val(3));


                // regs.hfxosccfg.modify(hfxosccfg::enable::SET);
                // regs.hfrosccfg.modify(hfrosccfg::enable::CLEAR );



                // // disable pll so that hfroscclk drives clock directly
                // regs.pllcfg.modify(pllcfg::sel::CLEAR);




                // // Enable external oscillator
                // regs.hfxosccfg.modify(hfxosccfg::enable::SET);
                // // Choose external oscillator for PLL
                // //
                // // When used to drive the PLL, the 16 MHz crystal oscillator
                // // output frequency must be divided by two in the first-stage
                // // divider of the PLL (i.e., ) to provide an 8 MHz reference
                // // clock to the VCO.
                // //
                // // R=2
                // // F=48
                // // Q=8
                // regs.pllcfg.write(pllcfg::pllr::R2 +
                //     pllcfg::pllf.val(23) +
                //     pllcfg::pllq.val(8) +
                //     pllcfg::bypass::CLEAR +
                //     pllcfg::sel::SET +
                //     pllcfg::refsel::SET);

                // // regs.plloutdiv.write(plloutdiv::div.val(0) + plloutdiv::divby1::SET);
                // regs.plloutdiv.write(plloutdiv::div.val(0) + plloutdiv::divby1::CLEAR);


            }
            ClockFrequency::Freq384Mhz => {}
        };
    }
}
