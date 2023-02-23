#include "system.h"

System::System(float start_sigma2, float start_rho, float start_nu2,
               float sigma2_const, float nu2_const, float min_rho,
               float min_sigma2, float min_nu2)
    : start_sigma2{start_sigma2}, start_rho{start_rho}, start_nu2{start_nu2},
      sigma2_const{sigma2_const}, nu2_const{nu2_const}, min_rho{min_rho},
      min_sigma2{min_sigma2}, min_nu2{min_nu2} {}