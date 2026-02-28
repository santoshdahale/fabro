use async_trait::async_trait;

use super::{Answer, Interviewer, Question};

/// Delegates question answering to a provided callback function.
pub struct CallbackInterviewer {
    callback: Box<dyn Fn(Question) -> Answer + Send + Sync>,
}

impl CallbackInterviewer {
    pub fn new(callback: impl Fn(Question) -> Answer + Send + Sync + 'static) -> Self {
        Self {
            callback: Box::new(callback),
        }
    }
}

#[async_trait]
impl Interviewer for CallbackInterviewer {
    async fn ask(&self, question: Question) -> Answer {
        (self.callback)(question)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interviewer::{AnswerValue, QuestionType};

    #[tokio::test]
    async fn calls_callback_with_question() {
        let interviewer = CallbackInterviewer::new(|q| {
            if q.question_type == QuestionType::YesNo {
                Answer::yes()
            } else {
                Answer::no()
            }
        });

        let yes_q = Question::new("approve?", QuestionType::YesNo);
        let answer = interviewer.ask(yes_q).await;
        assert_eq!(answer.value, AnswerValue::Yes);

        let no_q = Question::new("choose:", QuestionType::MultipleChoice);
        let answer = interviewer.ask(no_q).await;
        assert_eq!(answer.value, AnswerValue::No);
    }

    #[tokio::test]
    async fn callback_receives_question_text() {
        let interviewer = CallbackInterviewer::new(|q| Answer::text(q.text));
        let q = Question::new("hello world", QuestionType::Freeform);
        let answer = interviewer.ask(q).await;
        assert_eq!(answer.text, Some("hello world".to_string()));
    }
}
