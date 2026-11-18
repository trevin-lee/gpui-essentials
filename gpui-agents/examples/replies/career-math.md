### 2026-11-18 15:50 UTC · Career

Both are Kalman filters adapted for **nonlinear** systems.

**The plain Kalman filter, for reference.** It assumes everything is linear:
$$x_{k+1} = F x_k + w, \qquad z_k = H x_k + v$$
Here $x$ is the state you want (speed, slip angle, and so on), $z$ is what the sensors read, and $w, v$ are noise. Each step it **predicts** the state and its uncertainty (a covariance $P$), then **updates** using the measurement. The Kalman gain decides how much to trust the measurement versus the prediction. With linear equations and Gaussian noise, the uncertainty stays a clean Gaussian, so this is exact.

Real vehicles aren't linear. A Pacejka tyre force, for example, is a curved function of slip angle. So the motion and measurement equations become $x_{k+1} = f(x_k)$ and $z_k = h(x_k)$, with $f$ and $h$ nonlinear.

**EKF (Extended Kalman Filter).** It **linearizes** at each step. It takes the derivatives (Jacobians) of $f$ and $h$ at the current estimate and uses those straight-line approximations in place of $F$ and $H$.
- Pros: simple, fast, the standard workhorse.
- Cons: you need the derivatives, and if the system bends sharply near the operating point, the straight-line approximation is poor. The filter can then become overconfident or diverge.

**UKF (Unscented Kalman Filter).** Instead of linearizing, it **samples**. It picks a small set of "sigma points" around the current estimate, $2n+1$ points for an $n$-dimensional state, spread according to the uncertainty. It runs each point through the real nonlinear $f$ or $h$, then rebuilds the mean and covariance from the transformed points.
- Pros: no derivatives needed, handles curvature better (accurate to second order), and costs about the same as an EKF.
- Cons: a few tuning parameters for how the sigma points spread, and it still assumes the uncertainty is roughly Gaussian.

**An F1-flavored example.** Estimating sideslip angle, which is hard to measure directly, from yaw rate, lateral acceleration, steering angle and wheel speeds. You'd use a bicycle model with Pacejka tyres, which is nonlinear, so you'd reach for an EKF or a UKF. If the tyres sit near the peak of their force curve, where the curve bends most, the UKF often behaves better.

**One-line version for an interview:** "The EKF linearizes the model with Jacobians at each step. The UKF pushes a few sample points through the true nonlinear model instead, so it needs no derivatives and handles strong nonlinearity better at similar cost."

### 2026-11-18 15:51 UTC · Career

A **Jacobian** is the matrix of all first partial derivatives of a function with several inputs and several outputs. It's the multi-variable version of "the slope".

If $f$ takes $n$ inputs and gives $m$ outputs, the Jacobian $J$ is an $m \times n$ matrix:
$$J_{ij} = \frac{\partial f_i}{\partial x_j}$$
Row $i$ shows how output $i$ changes; column $j$ shows the effect of nudging input $j$.

**Why it matters: it's the best straight-line approximation near a point.**
$$f(x + \Delta x) \approx f(x) + J\,\Delta x$$
That's exactly what the EKF uses. It swaps the curved function for this local linear one at the current estimate, every step.

**Example.** Car velocity components from speed $v$ and heading $\theta$:
$$f(v,\theta) = \begin{pmatrix} v\cos\theta \\ v\sin\theta \end{pmatrix}
\quad\Rightarrow\quad
J = \begin{pmatrix} \cos\theta & -v\sin\theta \\ \sin\theta & v\cos\theta \end{pmatrix}$$
The first column is how the velocity components change if speed increases. The second is how they change if heading turns.

You've met it in physics already. When you change coordinates (Cartesian to polar), the determinant of the Jacobian is the $r$ in $r\,dr\,d\theta$: it tells you how areas stretch.

