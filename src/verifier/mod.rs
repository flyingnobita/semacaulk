use crate::constants::{NUMBER_OF_MIMC_ROUNDS, SUBGROUP_SIZE};
use crate::multiopen::verifier::Verifier as MultiopenVerifier;
use crate::prover::{Proof, PublicData};
use crate::transcript::Transcript;
use ark_bn254::{Bn254, Fq12, Fr, G1Affine, G2Affine};
use ark_ec::{AffineCurve, PairingEngine, ProjectiveCurve};
use ark_ff::Field;
use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
use ark_std::One;
use std::ops::Neg;

pub struct Verifier {}

impl Verifier {
    pub fn verify(
        proof: &Proof<Bn254>,
        a2_srs_g1: G1Affine,
        x_g2: G2Affine,
        accumulator: G1Affine,
        public_input: &PublicData<Bn254>,
    ) -> bool {
        let external_nullifier = public_input.external_nullifier;
        let nullifier_hash = public_input.nullifier_hash;
        let signal_hash = public_input.signal_hash;

        let mut transcript = Transcript::new_transcript();

        // Update transcript and derive challenges
        transcript.round_1(
            [
                &proof.commitments.w0,
                &proof.commitments.key,
                &proof.commitments.w1,
                &proof.commitments.w2,
            ],
            [external_nullifier, nullifier_hash, signal_hash],
        );

        let v = transcript.get_challenge();

        transcript.round_2([
            &proof.commitments.quotient,
            &proof.commitments.zi,
            &proof.commitments.ci,
            &proof.commitments.u_prime,
        ]);

        let _hi_1 = transcript.get_challenge();
        let hi_2 = transcript.get_challenge();

        transcript.round_3(&proof.commitments.w, &proof.commitments.h);

        let alpha = transcript.get_challenge();

        let domain_h = GeneralEvaluationDomain::new(SUBGROUP_SIZE).unwrap();

        let omega: Fr = domain_h.element(1);
        let omega_n = domain_h.element(NUMBER_OF_MIMC_ROUNDS);

        let omega_alpha = omega * alpha;
        let omega_n_alpha = omega_n * alpha;

        let key_openings = [proof.openings.key_0, proof.openings.key_1];

        let q_mimc_opening = proof.openings.q_mimc;
        let c_opening = proof.openings.c;
        let w0_openings = [
            proof.openings.w0_0,
            proof.openings.w0_1,
            proof.openings.w0_2,
        ];
        let w1_openings = [
            proof.openings.w1_0,
            proof.openings.w1_1,
            proof.openings.w1_2,
        ];

        let w2_openings = [
            proof.openings.w2_0,
            proof.openings.w2_1,
            proof.openings.w2_2,
        ];

        // Check if the gate equations are valid

        let l0_eval = domain_h.evaluate_all_lagrange_coefficients(alpha)[0];

        let n = domain_h.size();
        let test =
            (alpha.pow([n as u64, 0, 0, 0]) - Fr::one()) / Fr::from(n as u64) / (alpha - Fr::one());
        assert_eq!(test, l0_eval);

        // Compute the zh_eval - quotient_opening value, which is what the challenge-separated linear
        // combination of the gate evaluations should equal
        let pow_7 = |x: Fr| x.pow([7, 0, 0, 0]);

        // Gate 0: q_mimc_opening * ((w0_openings[0] + c_opening) ^ 7 - w0_openings[1])
        let gate_0_eval = q_mimc_opening * (pow_7(w0_openings[0] + c_opening) - w0_openings[1]);

        // Gate 1: q_mimc_opening * ((w1_openings[0] + key_openings[0] + c_opening) ^ 7 - w1_openings[1])
        let gate_1_eval =
            q_mimc_opening * (pow_7(w1_openings[0] + key_openings[0] + c_opening) - w1_openings[1]);

        // Gate 2:
        // q_mimc_opening * ((w2_openings[0] + key_openings[0] + c_opening) ^ 7 - w2_openings[1])
        let gate_2_eval =
            q_mimc_opening * (pow_7(w2_openings[0] + key_openings[0] + c_opening) - w2_openings[1]);

        // Gate 3:
        // q_mimc_opening * (key_openings[0] - key_openings[1])
        let gate_3_eval = q_mimc_opening * (key_openings[0] - key_openings[1]);

        // Gate 4:
        // l0 * (key_openings[0] - w0_openings[0] - w0_openings[2])
        let gate_4_eval = l0_eval * (key_openings[0] - w0_openings[0] - w0_openings[2]);

        // Gate 5:
        // l0 * (nullifierHash - w2_openings[0] - w2_openings[2] - (2 * key_openings[0]))
        let gate_5_eval = l0_eval
            * (nullifier_hash - w2_openings[0] - w2_openings[2] - (Fr::from(2) * key_openings[0]));

        // Gate 6:
        // l0 * (w2_openings[0] - external_nullifier)
        let gate_6_eval = l0_eval * (w2_openings[0] - external_nullifier);

        let v_pow_2 = v.pow([2, 0, 0, 0]);
        let v_pow_3 = v.pow([3, 0, 0, 0]);
        let v_pow_4 = v.pow([4, 0, 0, 0]);
        let v_pow_5 = v.pow([5, 0, 0, 0]);
        let v_pow_6 = v.pow([6, 0, 0, 0]);

        let lhs = gate_0_eval
            + (v * gate_1_eval)
            + (v_pow_2 * gate_2_eval)
            + (v_pow_3 * gate_3_eval)
            + (v_pow_4 * gate_4_eval)
            + (v_pow_5 * gate_5_eval)
            + (v_pow_6 * gate_6_eval);

        let zh_eval = alpha.pow([SUBGROUP_SIZE as u64, 0, 0, 0]) - Fr::one();
        let quotient_opening = proof.openings.quotient;
        let rhs = zh_eval * quotient_opening;

        if lhs != rhs {
            return false;
        }

        transcript.round_4([
            proof.openings.w0_0,
            proof.openings.w0_1,
            proof.openings.w0_2,
            proof.openings.w1_0,
            proof.openings.w1_1,
            proof.openings.w1_2,
            proof.openings.w2_0,
            proof.openings.w2_1,
            proof.openings.w2_2,
            proof.openings.key_0,
            proof.openings.key_1,
            q_mimc_opening,
            c_opening,
            proof.openings.quotient,
            proof.openings.u_prime,
            proof.openings.p1,
            proof.openings.p2,
        ]);

        let multiopen_final_poly = MultiopenVerifier::compute_final_poly(
            &mut transcript,
            &proof.multiopen_proof,
            &proof.commitments.w0,
            &[
                proof.openings.w0_0,
                proof.openings.w0_1,
                proof.openings.w0_2,
            ],
            &proof.commitments.w1,
            &[
                proof.openings.w1_0,
                proof.openings.w1_1,
                proof.openings.w1_2,
            ],
            &proof.commitments.w2,
            &[
                proof.openings.w2_0,
                proof.openings.w2_1,
                proof.openings.w2_2,
            ],
            &proof.commitments.key,
            &[proof.openings.key_0, proof.openings.key_1],
            &proof.commitments.q_mimc,
            q_mimc_opening,
            &proof.commitments.c,
            c_opening,
            &proof.commitments.quotient,
            quotient_opening,
            &proof.commitments.u_prime,
            proof.openings.u_prime,
            &proof.commitments.p1,
            proof.openings.p1,
            &proof.commitments.p2,
            proof.openings.p2,
            proof.openings.u_prime, //v,
            alpha,
            omega_alpha,
            omega_n_alpha,
        );

        // Perform this using product_of_pairings(): A * B * C and check that
        // the result equals Fq12::one().
        //
        // A: e(
        //   A1:  (C - ci) +
        //   A2:  (xi(x^n - 1)) +
        //   A3:  s * (zq - y + p),
        // [1])
        //
        // B: e(-zi,  w)
        //
        // C: e(-q * s, [x])
        //
        // A: [1] is E::G2Affine::prime_subgroup_generator()
        //   A1:
        //     C is accumulator
        //     ci is proof.commitments.ci
        //   A2:
        //     xi is hi_2
        //     (x^n - 1) is (public_input.srs_g1[common_input.domain_h.size()] + -E::G1Affine::prime_subgroup_generator())
        //   A3:
        //   s is the separator challenge
        //     zq is final_poly_proof.mul(x3)
        //     -y is g1.mul(final_poly_eval).neg()
        //     p is final_poly
        //
        // B:
        //   -zi is proof.commitments.zi.neg()
        //   w is w_commitment from caulk_second_round
        //
        // C:
        //   s is the separator challenge
        //   -q is final_poly_proof.neg()
        //   [x] is x_g2
        let s = transcript.get_challenge();

        let g1_gen = G1Affine::prime_subgroup_generator();
        let g2_gen = G2Affine::prime_subgroup_generator();
        let (final_poly, final_poly_eval, x3) = multiopen_final_poly;
        let final_poly_proof = proof.multiopen_proof.final_poly_proof;
        let minus_y = g1_gen.mul(final_poly_eval).neg();
        let zq = final_poly_proof.mul(x3);

        let a1 = accumulator + proof.commitments.ci.neg();
        let a2 = (a2_srs_g1 + g1_gen.neg()).mul(hi_2).into_affine();
        let a3 = (zq + minus_y)
            .add_mixed(&final_poly)
            .into_affine()
            .mul(s)
            .into_affine();

        let a_lhs = a1 + a2 + a3;
        let a_rhs = g2_gen;

        let b_lhs = proof.commitments.zi.neg();
        let b_rhs = proof.commitments.w;

        let c_lhs = final_poly_proof.neg().mul(s).into_affine();
        let c_rhs = x_g2;

        let res = Bn254::product_of_pairings(&[
            (a_lhs.into(), a_rhs.into()),
            (b_lhs.into(), b_rhs.into()),
            (c_lhs.into(), c_rhs.into()),
        ]);

        res == Fq12::one()
    }
}
